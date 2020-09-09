use std::collections::{HashMap, VecDeque};
use std::error::Error;

use select::document::Document;
use url::{ParseError, Url};

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
        Self {
            status,
            count,
        }
    }

    /// Increment the count by 1.
    fn increment(&mut self) {
        self.increment_by(1);
    }

    /// Increment the count by `n`.
    fn increment_by(&mut self, value: u32) {
        self.count = self.count + value;
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
struct Crawler<'a> {
    base: &'a Url,
    domain_only: bool,
    limit: u32,
    pending: VecDeque<String>,
    responses: HashMap<String, Response>,
}

impl<'a> Crawler<'a> {
    /// Create a new crawler instance.
    fn new(base: &'a Url, domain_only: bool) -> Self {
        Self {
            base,
            domain_only,
            limit: 0,
            pending: VecDeque::new(),
            responses: HashMap::new(),
        }
    }

    /// Limit the number of links to crawl.
    fn set_limit(&mut self, limit: u32) -> &Self {
        self.limit = limit;
        self
    }

    /// Handle a response (after the request get executed).
    async fn on_response(&mut self, response: reqwest::Response) -> Result<(), Box<dyn Error>> {
        let url = response.url().to_string();
        let status = response.status().as_u16();
        let body = response.text().await?;

        Document::from(body.as_str())
            .find(select::predicate::Name("a"))
            .filter_map(|n| n.attr("href"))
            .for_each(|x| self.queue(x));

        println!("{} - {}", status, url);

        self.responses
            .entry(url)
            .or_insert(Response::new(status, 1))
            .set_status(status);

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

        if url.starts_with("/") {
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
        if self.limit > 0 && self.responses.len() >= self.limit as usize {
            return true;
        }

        if let Some(entry) = self.responses.get_mut(url) {
            entry.increment();
            return true;
        }

        match Url::parse(url) {
            // If we successfully parse the URL, we can easily check if the domain is different than
            // requested.
            Ok(entry) => self.domain_only && entry.domain() != self.base.domain(),

            // Otherwise, we only have to handle relative URL. If error is about relative, we can
            // safely not exclude the URL because we're working using absolute format.
            Err(e) => match e {
                ParseError::RelativeUrlWithoutBase => false,
                _ => true,
            }
        }
    }

    /// Execute a request using the given URL.
    async fn execute(&mut self, url: &str) -> Result<(), Box<dyn Error>> {
        self
            .on_response(
                reqwest::get(&self.format_url(url)).await?
            )
            .await
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let url = Url::parse("https://lesiteimmo.test").expect("Cannot parse the given initial URL.");
    let mut crawler = Crawler::new(&url, true);

    crawler.set_limit(100);
    crawler.queue(url.path());

    loop {
        if let Some(url) = crawler.pending.pop_front() {
            crawler.execute(url.as_str()).await?;
        } else {
            break;
        }
    }

    println!("{:#?}", crawler);
    Ok(())
}
