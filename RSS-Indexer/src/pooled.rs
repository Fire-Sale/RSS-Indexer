use rss::Channel;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::result::Result;

use std::sync::{Arc, Mutex};
use url::Url;

use crate::common::*;
use crate::threadpool::*;

/// Thread pool sizes.
const SIZE_FEEDS_POOL: usize = 3; //3
const SIZE_SITES_POOL: usize = 20; //20

/// Same as the single/multi threaded version, but using a thread pool. Set up two thread pools:
/// one for handling feeds, and one for handling articles. Use the sizes above. Push closures
/// executing `process_feed` into the thread pool.
pub fn process_feed_file(file_name: &str, index: Arc<Mutex<ArticleIndex>>) -> RssIndexResult<()> {
    // todo!()
    let file = File::open(file_name)?;
    println!("Processing feed file: {}", file_name);

    let mut feeds_pool = ThreadPool::new(SIZE_FEEDS_POOL);
    let sites_pool = Arc::new(Mutex::new(ThreadPool::new(SIZE_SITES_POOL)));

    let channel = Channel::read_from(BufReader::new(file))?;
    let urls = Arc::new(Mutex::new(HashSet::new()));

    for feed in channel.into_items() {
        let url = feed.link().ok_or(RssIndexError::UrlError)?;
        let title = feed.title().ok_or(RssIndexError::UrlError)?;

        if urls.lock().unwrap().contains(url) {
            println!("Skipping already seen feed: {} [{}]", title, url);
            continue;
        }
        urls.lock().unwrap().insert(url.to_string());

        println!("Processing feed: {} [{}]", title, url);

        let urls = Arc::clone(&urls);
        let index = Arc::clone(&index);
        let sites_pool = Arc::clone(&sites_pool);
        let url = url.to_string();
        feeds_pool.execute(move || {
            process_feed(&url, index, urls, sites_pool).unwrap();
        })
    }

    Result::Ok(())
}

/// Same as the single/multi threaded version, but using a thread pool. Push closures executing
/// `process_article` into the thread pool that is passed in.
fn process_feed(
    url: &str,
    index: Arc<Mutex<ArticleIndex>>,
    urls: Arc<Mutex<HashSet<String>>>,
    sites_pool: Arc<Mutex<ThreadPool>>,
) -> RssIndexResult<()> {
    // todo!()
    let contents = reqwest::blocking::get(url)?.bytes()?;
    let channel = Channel::read_from(&contents[..])?;
    let items = channel.into_items();
    for item in items {
        let (url, site, title) = match (item.link(), Url::parse(&url)?.host_str(), item.title()) {
            (Some(u), Some(s), Some(t)) => (u, s.to_string(), t),
            _ => continue,
        };

        if urls.lock().unwrap().contains(url) {
            println!("Skipping already seen article: {} [{}]", title, url);
            continue;
        }
        urls.lock().unwrap().insert(url.to_string());

        println!("Processing article: {} [{}]", title, url);

        let article = Article::new(url.to_string(), title.to_string());

        let sites_pool = Arc::clone(&sites_pool);
        let mut sites_pool = sites_pool.lock().unwrap();
        let index = Arc::clone(&index);

        let url = url.to_string();
        let title = title.to_string();
        sites_pool.execute(move || {
            let article_words = process_article(&article);
            index.lock().unwrap().add(
                site.to_string(),
                title.to_string(),
                url.to_string(),
                article_words.unwrap(),
            );
        });
    }
    Result::Ok(())
}
