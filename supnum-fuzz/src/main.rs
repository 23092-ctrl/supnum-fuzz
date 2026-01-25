use clap::Parser;

use colored::*;

use futures::stream::{self, StreamExt};

use reqwest::{Client, StatusCode};

use std::sync::Arc;

use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};

use std::time::{Duration, Instant};

use std::io::{BufRead, BufReader};

use std::fs::File;

use std::pin::Pin;

use futures::Future;

use indicatif::{ProgressBar, ProgressStyle};


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

    #[arg(short = 'r', long, default_value_t = 1)]

    recurse: usize,

    #[arg(long = "fs")]

    filter_size: Option<String>,

    #[arg(short = 'e', long)]

    exclude: Option<String>,

    #[arg(short = 't', long, default_value_t = 100)]

    threads: usize,

}


struct AppContext {

    client: Client,

    extensions: Vec<String>,

    exclude_codes: Vec<u16>,

    filter_sizes: Vec<u64>,

    args: Args,

    progress: ProgressBar,

    current_threads: AtomicUsize,

    avg_latency: AtomicU64,

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


    // 3. Client HTTP haute performance

    let mut headers = reqwest::header::HeaderMap::new();

    headers.insert("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36".parse().unwrap());


    let client = Client::builder()

    .default_headers(headers)

    .tcp_nodelay(true)

    .pool_max_idle_per_host(args.threads)

    .tcp_keepalive(Duration::from_secs(120))

    .danger_accept_invalid_certs(true)

    .redirect(reqwest::redirect::Policy::none())

    .build().unwrap();


    let ctx = Arc::new(AppContext {

        client,

        extensions,

        exclude_codes,

        filter_sizes,

        args: args.clone(),

                       progress: pb,

                       current_threads: AtomicUsize::new(0),

                       avg_latency: AtomicU64::new(0),

    });


    println!("{}", "🚀 Scan Ultra-Rapide (Zéro Attente au Lancement)".bold().cyan());

    scan_url(ctx.clone(), args.url.clone(), 1).await;

    ctx.progress.finish_with_message("Terminé.");

}


fn scan_url(ctx: Arc<AppContext>, base_url: String, current_depth: usize) -> BoxFuture<'static, ()> {

    Box::pin(async move {

        let file = match File::open(&ctx.args.wordlist) {

            Ok(f) => f,

             Err(_) => return,

        };


        let reader = BufReader::with_capacity(512 * 1024, file);

        let has_fuzz = base_url.contains("FUZZ");

        let clean_base = base_url.trim_end_matches('/').to_string();


        let url_stream = stream::iter(reader.lines().filter_map(|l| l.ok()))

        .flat_map(|word| {

            let mut variants = Vec::with_capacity(ctx.extensions.len() + 1);

            let w = word.trim();

            if w.is_empty() || w.starts_with('#') { return stream::iter(variants); }


            let target = if has_fuzz {

                base_url.replace("FUZZ", w)

            } else {

                let mut s = String::with_capacity(clean_base.len() + w.len() + 1);

                s.push_str(&clean_base);

                s.push('/');

                s.push_str(w);

                s

            };


            variants.push(target.clone());

            for ext in &ctx.extensions {

                let mut s = String::with_capacity(target.len() + ext.len() + 1);

                s.push_str(&target);

                s.push('.');

                s.push_str(ext);

                variants.push(s);

            }

            stream::iter(variants)

        });




        let results = url_stream

        .map(|url| {

            let ctx = ctx.clone();

            async move {

                let start = Instant::now();

                let res = ctx.client.get(&url).send().await;

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

                    let dur_ms = duration.as_millis() as u64;




                    let old_avg = ctx.avg_latency.load(Ordering::Relaxed);

                    ctx.avg_latency.store(if old_avg == 0 { dur_ms } else { (old_avg + dur_ms) / 2 }, Ordering::Relaxed);


                    if !ctx.exclude_codes.contains(&code) {

                        let len = resp.content_length().unwrap_or(0);

                        if !ctx.filter_sizes.contains(&len) {

                            ctx.progress.suspend(|| {

                                println!("[{}] {:>8} | {}", code.to_string().green(), len, url);

                            });




                            if current_depth < ctx.args.recurse && is_directory(status, &url) {

                                let new_base = if url.ends_with('/') { url } else { format!("{}/", url) };

                                tokio::spawn(scan_url(ctx.clone(), new_base, current_depth + 1));

                            }

                        }

                    }

                }

                ctx.progress.set_message(format!("Latence: {}ms", ctx.avg_latency.load(Ordering::Relaxed)));

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

