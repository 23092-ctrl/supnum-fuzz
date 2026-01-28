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

    #[arg(long = "fs")]
    filter_size: Option<String>,

    #[arg(short = 'e', long)]
    exclude: Option<String>,

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
    ignore_lines: Option<usize>,
    ignore_words: Option<usize>,
}

struct AppContext {
    client: Client,
    extensions: Vec<String>,
    exclude_codes: Vec<u16>,
    filter_sizes: Vec<u64>,
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

    let exclude_codes: Vec<u16> = args.exclude.as_deref().unwrap_or("404")
        .split(',').filter_map(|s| s.parse().ok()).collect();

    let filter_sizes: Vec<u64> = args.filter_size.as_deref().unwrap_or("")
        .split(',').filter_map(|s| s.parse().ok()).collect();

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".parse().unwrap());

    let client = Client::builder()
        .default_headers(headers)
        .tcp_nodelay(true)
        .pool_max_idle_per_host(args.threads)
        .tcp_keepalive(Duration::from_secs(60))
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build().unwrap();

  
    let mut calibration = Calibration { is_active: false, ignore_len: None, ignore_lines: None, ignore_words: None };
    
    if args.smart {
        pb.set_message("Calibrating 404 signature...");
        if let Some(cal) = calibrate(&client, &args.url).await {
            calibration = cal;
            let msg = format!("Smart Filter: Ignoring Len: {:?}, Lines: {:?}", calibration.ignore_len, calibration.ignore_lines);
            pb.println(msg.yellow().to_string());
        } else {
            pb.println("Calibration failed or site unstable.".red().to_string());
        }
    }


    let ctx = Arc::new(AppContext {
        client,
        extensions,
        exclude_codes,
        filter_sizes,
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

async fn calibrate(client: &Client, base_url: &str) -> Option<Calibration> {

    let random_path: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(12)
        .map(char::from)
        .collect();

    let target = if base_url.ends_with('/') {
        format!("{}{}", base_url, random_path)
    } else {
        format!("{}/{}", base_url, random_path)
    };

    if let Ok(resp) = client.get(&target).send().await {
        let len = resp.content_length().unwrap_or(0);
        
        if let Ok(text) = resp.text().await {
            let lines = text.lines().count();
            let words = text.split_whitespace().count();
            
            return Some(Calibration {
                is_active: true,
                ignore_len: Some(len),
                ignore_lines: Some(lines),
                ignore_words: Some(words),
            });
        }
    }
    None
}

fn scan_url(ctx: Arc<AppContext>, base_url: String, current_depth: usize) -> BoxFuture<'static, ()> {
    Box::pin(async move {
        let file = match File::open(&ctx.args.wordlist) {
            Ok(f) => f,
            Err(_) => return,
        };

        let reader = BufReader::with_capacity(64 * 1024, file); // Reduced buffer slightly to save RAM
        let clean_base = base_url.trim_end_matches('/').to_string();
        let has_fuzz = base_url.contains("FUZZ");

        
        let url_stream = stream::iter(reader.lines().filter_map(|l| l.ok()))
            .flat_map(|word| {
                let w = word.trim();
                let mut variants = Vec::with_capacity(ctx.extensions.len() + 1);
                
                if w.is_empty() || w.starts_with('#') { return stream::iter(variants); }

                let target = if has_fuzz {
                    base_url.replace("FUZZ", w)
                } else {
                    format!("{}/{}", clean_base, w)
                };

                variants.push(target.clone());
                for ext in &ctx.extensions {
                    variants.push(format!("{}.{}", target, ext));
                }
                stream::iter(variants)
            });

        let results = url_stream
            .map(|url| {
                let ctx = ctx.clone();
                async move {
                 
                    let _permit = ctx.semaphore.acquire().await.unwrap();

                   
                    if ctx.args.jitter > 0 {
                        let delay = rand::thread_rng().gen_range(0..ctx.args.jitter);
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }

                    let start = Instant::now();
                    
                    let mut res = ctx.client.request(Method::HEAD, &url).send().await;
                    
                    if let Ok(ref r) = res {
                        if r.status() == StatusCode::METHOD_NOT_ALLOWED {
                            res = ctx.client.get(&url).send().await;
                        }
                    }

                    (url, res, start.elapsed())
                }
            })
           
            .buffer_unordered(ctx.args.threads);

        results.for_each(|(url, res, duration)| {
            let ctx = ctx.clone();
            async move {
                ctx.progress.inc(1);

                if let Ok(resp) = res {
                    let status = resp.status();
                    let code = status.as_u16();
                    let len = resp.content_length().unwrap_or(0);

                  
                    let dur_ms = duration.as_millis() as u64;
                    let old_avg = ctx.avg_latency.load(Ordering::Relaxed);
                    ctx.avg_latency.store(if old_avg == 0 { dur_ms } else { (old_avg + dur_ms) / 2 }, Ordering::Relaxed);
                    ctx.progress.set_message(format!("Latence: {}ms", ctx.avg_latency.load(Ordering::Relaxed)));

                
                    if ctx.exclude_codes.contains(&code) { return; }
                    
               
                    if ctx.filter_sizes.contains(&len) { return; }

                    if ctx.calibration.is_active {
                        if let Some(ignore_len) = ctx.calibration.ignore_len {
                      
                            if len == ignore_len { return; }
                        
                             if len > 0 && (len as i64 - ignore_len as i64).abs() < 5 { return; }
                        }
                    }

                  
                    ctx.progress.suspend(|| {
                        let code_colored = match code {
                            200..=299 => code.to_string().green(),
                            300..=399 => code.to_string().blue(),
                            400..=499 => code.to_string().yellow(),
                            500..=599 => code.to_string().red(),
                            _ => code.to_string().white(),
                        };
                        
                        println!("[{}] {:>8} | {}", code_colored, len.to_string().dimmed(), url);
                    });

                    if current_depth < ctx.args.recurse && is_directory(status, &url) {
                         let new_base = if url.ends_with('/') { url } else { format!("{}/", url) };
                       
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
