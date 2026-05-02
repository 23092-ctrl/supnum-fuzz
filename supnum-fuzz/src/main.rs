use clap::Parser;
use colored::*;
use futures::stream::{self, StreamExt};
use reqwest::{Client, Method, StatusCode};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::io::{BufRead, BufReader};
use std::fs::File;
use std::pin::Pin;
use futures::Future;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::sync::Semaphore;
use rand::Rng;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(short = 'u', long)]
    url: String,

    #[arg(short = 'w', long)]
    wordlist: String,

    #[arg(short = 'x', long)]
    extensions: Option<String>,

    #[arg(short = 'r', long, default_value_t = 0)]
    recurse: usize,

    #[arg(short = 'H', long = "header")]
    headers: Vec<String>,

    #[arg(long = "mc", default_value = "200,204,301,302,307,401,403,405")]
    match_codes: String,

    #[arg(short = 'e', long)]
    exclude: Option<String>,

    #[arg(long = "fs")]
    filter_size: Option<String>,

    #[arg(long = "fw")]
    filter_words: Option<String>,

    #[arg(short = 't', long, default_value_t = 100)]
    threads: usize,

    #[arg(long, default_value_t = 0)]
    jitter: u64,

    #[arg(long, default_value_t = false)]
    smart: bool,
}

#[derive(Debug, Clone)]
struct Calibration {
    is_active: bool,
    ignore_len: Option<u64>,
    ignore_words: Option<usize>,
}

struct AppContext {
    client: Client,
    extensions: Vec<String>,
    headers_template: Vec<(String, String)>,
    match_codes: Vec<u16>,
    exclude_codes: Vec<u16>,
    filter_sizes: Vec<u64>,
    filter_words: Vec<usize>,
    args: Args,
    progress: ProgressBar,
    avg_latency: AtomicU64,
    calibration: Calibration,
    semaphore: Arc<Semaphore>, 
}

#[tokio::main]
async fn main() {
    show("Cheikh ELghadi", "https://github.com/23092-ctrl");
    let args = Args::parse();

    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .template("{spinner:.magenta} [{elapsed_precise}] {pos} reqs ({per_sec}) | {msg}")
        .unwrap());

  
    let extensions: Vec<String> = args.extensions.as_deref().unwrap_or("")
        .split(',').filter(|s| !s.is_empty())
        .map(|s| s.trim().trim_start_matches('.').to_string()).collect();

    let match_codes: Vec<u16> = args.match_codes
        .split(',').filter_map(|s| s.parse().ok()).collect();

    let exclude_codes: Vec<u16> = args.exclude.as_deref().unwrap_or("")
        .split(',').filter_map(|s| s.parse().ok()).collect();

    let filter_sizes: Vec<u64> = args.filter_size.as_deref().unwrap_or("")
        .split(',').filter_map(|s| s.parse().ok()).collect();

    let filter_words: Vec<usize> = args.filter_words.as_deref().unwrap_or("")
        .split(',').filter_map(|s| s.parse().ok()).collect();

    
    let mut headers_template = Vec::new();
    for h in &args.headers {
        if let Some((k, v)) = h.split_once(':') {
            headers_template.push((k.trim().to_string(), v.trim().to_string()));
        }
    }

    let mut default_headers = reqwest::header::HeaderMap::new();
    default_headers.insert("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36".parse().unwrap());

    let client = Client::builder()
        .default_headers(default_headers)
        .tcp_nodelay(true)
        .pool_max_idle_per_host(args.threads)
        .tcp_keepalive(Duration::from_secs(60))
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build().unwrap();

    let mut calibration = Calibration { is_active: false, ignore_len: None, ignore_words: None };
    

    if args.smart {
        pb.set_message("Calibrating 404 signature...");
    
    }

    let ctx = Arc::new(AppContext {
        client,
        extensions,
        headers_template,
        match_codes,
        exclude_codes,
        filter_sizes,
        filter_words,
        args: args.clone(),
        progress: pb,
        avg_latency: AtomicU64::new(0),
        calibration,
        semaphore: Arc::new(Semaphore::new(args.threads)),
    });

    println!("{}", "🚀 Scan Ultra-Rapide (Enhanced Mode)".bold().cyan());
    
    scan_url(ctx.clone(), args.url.clone(), 0).await;
    
    ctx.progress.finish_with_message("Terminé.");
}

fn scan_url(ctx: Arc<AppContext>, base_url: String, current_depth: usize) -> BoxFuture<'static, ()> {
    Box::pin(async move {
        let file = match File::open(&ctx.args.wordlist) {
            Ok(f) => f,
            Err(_) => return,
        };

        let reader = BufReader::with_capacity(64 * 1024, file);
        let clean_base = base_url.trim_end_matches('/').to_string();
        
        let url_has_fuzz = base_url.contains("FUZZ");
        let headers_have_fuzz = ctx.headers_template.iter().any(|(_, v)| v.contains("FUZZ"));

        let url_stream = stream::iter(reader.lines().filter_map(|l| l.ok()))
            .flat_map(|word| {
                let w = word.trim();
                let mut variants = Vec::with_capacity(ctx.extensions.len() + 1);
                
                if w.is_empty() || w.starts_with('#') { return stream::iter(variants); }

                let target = if url_has_fuzz {
                    base_url.replace("FUZZ", w)
                } else if headers_have_fuzz {
                    base_url.clone() 
                } else {
                    format!("{}/{}", clean_base, w)
                };

                variants.push((target.clone(), w.to_string()));
                for ext in &ctx.extensions {
                    variants.push((format!("{}.{}", target, ext), w.to_string()));
                }
                stream::iter(variants)
            });

        let results = url_stream
            .map(|(url, word)| {
                let ctx = ctx.clone();
                async move {
                    let _permit = ctx.semaphore.acquire().await.unwrap();

                    if ctx.args.jitter > 0 {
                        let delay = rand::thread_rng().gen_range(0..ctx.args.jitter);
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }

                    let start = Instant::now();
                    
                   
                    let mut req_builder = ctx.client.request(Method::GET, &url);

                  
                    for (k, v) in &ctx.headers_template {
                        let final_v = v.replace("FUZZ", &word);
                        req_builder = req_builder.header(k, final_v);
                    }

                    let res = req_builder.send().await;

                    (url, word, res, start.elapsed())
                }
            })
            .buffer_unordered(ctx.args.threads);

        results.for_each(|(url, word, res, duration)| {
            let ctx = ctx.clone();
            async move {
                ctx.progress.inc(1);

                if let Ok(resp) = res {
                    let status = resp.status();
                    let code = status.as_u16();
                    
                
                    if !ctx.match_codes.contains(&code) { return; }
                    if ctx.exclude_codes.contains(&code) { return; }

                    let mut len = resp.content_length().unwrap_or(0);
                    let mut words_count = 0;

                
                    let needs_body = !ctx.filter_words.is_empty() || ctx.calibration.is_active;
                    
                    if needs_body {
                        if let Ok(bytes) = resp.bytes().await {
                            len = bytes.len() as u64;
                            let text = String::from_utf8_lossy(&bytes);
                            words_count = text.split_whitespace().count();
                        }
                    }

                    let dur_ms = duration.as_millis() as u64;
                    let old_avg = ctx.avg_latency.load(Ordering::Relaxed);
                    ctx.avg_latency.store(if old_avg == 0 { dur_ms } else { (old_avg + dur_ms) / 2 }, Ordering::Relaxed);
                    ctx.progress.set_message(format!("Latence: {}ms", ctx.avg_latency.load(Ordering::Relaxed)));

                  
                    if ctx.filter_sizes.contains(&len) { return; }
                    if !ctx.filter_words.is_empty() && ctx.filter_words.contains(&words_count) { return; }

                    ctx.progress.suspend(|| {
                        let code_colored = match code {
                            200..=299 => code.to_string().green(),
                            300..=399 => code.to_string().blue(),
                            400..=499 => code.to_string().yellow(),
                            500..=599 => code.to_string().red(),
                            _ => code.to_string().white(),
                        };
                        
                        let display_target = if ctx.headers_template.iter().any(|(_, v)| v.contains("FUZZ")) {
                            format!("{} (Payload: {})", url, word.cyan())
                        } else {
                            url.clone()
                        };

                        let stats = format!("Size: {:>6} | Words: {:>4}", len.to_string().dimmed(), words_count.to_string().dimmed());
                        println!("[{}] {} | {}", code_colored, stats, display_target);
                    });

                    if current_depth < ctx.args.recurse && is_directory(status, &url) {
                         let new_base = if url.ends_with('/') { url.clone() } else { format!("{}/", url) };
                         tokio::spawn(scan_url(ctx.clone(), new_base, current_depth + 1));
                    }
                }
            }
        }).await;
    })
}

fn is_directory(status: StatusCode, url: &str) -> bool {
    status.is_redirection() || (status == StatusCode::OK && !url.split('/').last().unwrap_or("").contains('.'))
}

pub fn show(name: &str, github: &str) {
    println!("{}", r#"
         111111111    11      11    111111011
        11            11      11    11     10
        11            11      11    11     10
         11111111     11      11    111110101
                11    11      11    11
                11    11      11    11
        111111111      10010111     11

        11      11    11      11    11      11
        111     11    11      11    111    010
        11 11   11    11      11    11 1111 10
        11  11  11    11      11    11  10  01
        11   11 11    11      11    11      10
        11     111    11      11    11      01
        11      11     11111111     11      10
"#.bright_green());

    println!("{}", format!("        Institut Supérieur du Numérique — by {}", name).bright_white());
    println!("{}", format!("        GitHub : {}", github).bright_blue());
    println!("{}", "------------------------------------------------------------".bright_black());
    println!();
}
