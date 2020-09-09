#[macro_use]
extern crate log;

use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::io::Write;
use std::sync::Arc;

use clap::Clap;
use env_logger::Env;
use futures::lock::Mutex;
use select::document::Document;
use url::{ParseError, Url};

#[derive(Clap, Debug)]
#[clap(version = "1.0", author = "Cyril Mizzi <me@p1ngouin.com>")]
struct Opts {
    /// The domain to start working on.
    entrypoint: String,

    /// Crawler will only execute URLs from the same domain as <entrypoint>.
    #[clap(short, long)]
    restrict_on_domain: bool,

    /// Limit the number of URL to crawl.
    #[clap(short, long, default_value = "0")]
    limit: u32,

    /// Verbosity. By default, will only log ERROR level.
    #[clap(short, long, parse(from_occurrences))]
    verbose: i32,
}

/// A response can be an already-parsed URL or even a in-progress URL.
///
/// This struct is really important because it counts the number of time a link if found and also
/// stores the response status code in order to summarize informations.
#[derive(Debug)]
struct Response {
    /// Keep track number of found links.
    count: u32,

    /// Keep track of response status code.
    status: u16,
}

impl Response {
    /// Create a new Response instance.
    fn new(status: u16, count: u32) -> Self {
        Self { status, count }
    }

    /// Increment the count by 1.
    fn increment(&mut self) {
        self.increment_by(1);
    }

    /// Increment the count by `n`.
    fn increment_by(&mut self, value: u32) {
        self.count += value;
    }

    /// Update the status.
    fn set_status(&mut self, status: u16) {
        self.status = status;
    }
}

/// Crawler handles responses, pending queue and other cool stuffs.
///
/// This struct is the base of our crawler. It handles the base URL and other options.
#[derive(Debug)]
struct Crawler {
    base: Url,
    opts: Opts,
    pending: VecDeque<String>,
    responses: HashMap<String, Response>,
}

impl Crawler {
    /// Create a new crawler instance.
    fn new(opts: Opts) -> Self {
        let url = Url::parse(&opts.entrypoint).expect("Cannot parse the given initial URL.");
        let mut crawler = Self {
            base: url.clone(),
            opts,
            pending: VecDeque::new(),
            responses: HashMap::new(),
        };

        crawler.queue(url.path());
        crawler
    }

    /// Handle a response (after the request get executed).
    async fn on_response(&mut self, response: reqwest::Response) -> Result<(), Box<dyn Error>> {
        let url = response.url().clone();
        let status = response.status();
        let body = response.text().await?;

        Document::from(body.as_str())
            .find(select::predicate::Name("a"))
            .filter_map(|n| n.attr("href"))
            .for_each(|x| self.queue(x));

        if status.is_success() {
            info!("{} - {}", status, url);
        } else {
            error!("{} - {}", status, url);
        }

        self.responses
            .entry(url.to_string())
            .or_insert_with(|| Response::new(status.as_u16(), 1))
            .set_status(status.as_u16());

        Ok(())
    }

    /// Queue a new URL.
    ///
    /// This method is smart enough to prevent duplication. A link is always pushed once.
    fn queue(&mut self, url: &str) {
        let url = self.format_url(url);

        if self.is_excluded(&url) {
            return;
        }

        self.responses.insert(url.clone(), Response::new(0, 1));
        self.pending.push_back(url);
    }

    /// Format an URL.
    ///
    /// When an URL is relative, we have to complete it with our base. If the URL is absolute,
    /// simply return.
    fn format_url(&self, url: &str) -> String {
        let mut formatted: String = url.to_string();

        if url.starts_with('/') {
            if let Ok(full_url) = self.base.join(url) {
                formatted = full_url.to_string();
            } else {
                formatted = format!("{}{}", self.base.to_string(), url.to_string());
            }
        }

        formatted
    }

    /// Check if an URL should be excluded (already in progress or not on the domain).
    fn is_excluded(&mut self, url: &str) -> bool {
        if self.opts.limit > 0 && self.responses.len() >= self.opts.limit as usize {
            return true;
        }

        if let Some(entry) = self.responses.get_mut(url) {
            entry.increment();
            return true;
        }

        match Url::parse(url) {
            // If we successfully parse the URL, we can easily check if the domain is different than
            // requested.
            Ok(entry) => self.opts.restrict_on_domain && entry.domain() != self.base.domain(),

            // Otherwise, we only have to handle relative URL. If error is about relative, we can
            // safely not exclude the URL because we're working using absolute format.
            Err(e) => match e {
                ParseError::RelativeUrlWithoutBase => false,
                _ => true,
            },
        }
    }

    /// Execute a request using the given URL.
    async fn execute(&mut self, url: &str) -> Result<(), Box<dyn Error>> {
        self.on_response(reqwest::get(&self.format_url(url)).await?)
            .await
    }
}

/// Initialize the logger.
fn init_logger(opts: &Opts) {
    let env = Env::default().default_filter_or(
        match opts.verbose {
            0 => "error",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    );

    env_logger::from_env(env)
        .format(|buf, record| {
            let level_style = buf.default_level_style(record.level());
            writeln!(buf, "[{} {:>5}]: {}", buf.timestamp(), level_style.value(record.level()), record.args())
        })
        .init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let opts: Opts = Opts::parse();

    init_logger(&opts);

    let crawler = Arc::new(Mutex::new(Crawler::new(opts)));
    let mut threads = vec![];

    for _ in 0..5 {
        let crawler = Arc::clone(&crawler);

        threads.push(tokio::spawn(async move {
            loop {
                let mut crawler = crawler.lock().await;

                if let Some(url) = crawler.pending.pop_front() {
                    if let Err(e) = crawler.execute(&url).await {
                        eprintln!("{}", e);
                    }
                } else {
                    break;
                }
            }
        }));
    }

    futures::future::join_all(threads).await;
    Ok(())
}
