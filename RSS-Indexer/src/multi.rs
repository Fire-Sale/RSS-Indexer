use rss::Channel;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::result::Result;

use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use url::Url;

use crate::common::*;

/// Thread limits.
const MAX_THREADS_FEEDS: u32 = 5;
const MAX_THREADS_SITES: u32 = 10; // 10
const MAX_THREADS_TOTAL: u32 = 18; // 18

/// A lock around some T, with a condition variable for notifying/waiting.
struct CvarLock<T> {
    mutex: Mutex<T>,
    condvar: Condvar,
}

impl<T> CvarLock<T> {
    fn new(data: T) -> Self {
        let mutex = Mutex::new(data);
        let condvar = Condvar::new();
        CvarLock { mutex, condvar }
    }
}

/// Locks/Condvars around counters, tracking the number of feed threads, the number of article
/// threads per hostname, and the total number of threads.
pub struct ThreadCount {
    feeds_count: CvarLock<u32>,
    sites_count: CvarLock<HashMap<String, u32>>,
    total_count: CvarLock<u32>,
}

/// Same as for the single-threaded version, but now spawn a new thread for each call to
/// `process_feed`. Make sure to respect the thread limits!
pub fn process_feed_file(file_name: &str, index: Arc<Mutex<ArticleIndex>>) -> RssIndexResult<()> {
    let file = File::open(file_name)?;
    println!("Processing feed file: {}", file_name);

    let channel = Channel::read_from(BufReader::new(file))?;
    let urls = Arc::new(Mutex::new(HashSet::new()));

    let mut handles = Vec::new();
    let tc = Arc::new(ThreadCount {
        feeds_count: CvarLock::new(0),
        sites_count: CvarLock::new(HashMap::new()),
        total_count: CvarLock::new(0),
    });

    for feed in channel.into_items() {
        let url = feed.link().ok_or(RssIndexError::UrlError)?;
        let title = feed.title().ok_or(RssIndexError::UrlError)?;

        if urls.lock().unwrap().contains(url) {
            println!("Skipping already seen feed: {} [{}]", title, url);
            continue;
        }
        urls.lock().unwrap().insert(url.to_string());
        println!("Processing feed: {} [{}]", title, url);

        {
            let mut cur_tot_cnt = tc.total_count.mutex.lock().unwrap();
            while *cur_tot_cnt > MAX_THREADS_TOTAL - 1 {
                cur_tot_cnt = tc.total_count.condvar.wait(cur_tot_cnt).unwrap();
            }
            *cur_tot_cnt += 1;
        }

        {
            let mut cur_feeds_cnt = tc.feeds_count.mutex.lock().unwrap();
            while *cur_feeds_cnt > MAX_THREADS_FEEDS - 1 {
                cur_feeds_cnt = tc.feeds_count.condvar.wait(cur_feeds_cnt).unwrap();
            }
            *cur_feeds_cnt += 1;
        }

        let tc2 = Arc::clone(&tc);
        let url = url.to_string();
        let urls = Arc::clone(&urls);
        let index = Arc::clone(&index);

        let handle = thread::spawn(move || {
            let tc3 = Arc::clone(&tc2);
            process_feed(&url, index, urls, tc2).unwrap();

            {
                let mut cur_tot_cnt = tc3.total_count.mutex.lock().unwrap();
                *cur_tot_cnt -= 1;
                tc3.total_count.condvar.notify_one();
            }

            {
                let mut cur_feeds_cnt = tc3.feeds_count.mutex.lock().unwrap();
                *cur_feeds_cnt -= 1;
                tc3.feeds_count.condvar.notify_one();
            }
        });

        handles.push(handle);
    }
    for handle in handles {
        handle.join().unwrap();
    }
    Result::Ok(())
}

/// Same as for the single-threaded version, but now spawn a new thread for each call to
/// `process_article`. Make sure to respect the thread limits!
fn process_feed(
    url: &str,
    index: Arc<Mutex<ArticleIndex>>,
    urls: Arc<Mutex<HashSet<String>>>,
    counters: Arc<ThreadCount>,
) -> RssIndexResult<()> {
    let contents = reqwest::blocking::get(url)?.bytes()?;
    let channel = Channel::read_from(&contents[..])?;
    let items = channel.into_items();
    let mut handles = Vec::new();
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

        {
            let mut cur_tot_cnt = counters.total_count.mutex.lock().unwrap();
            while *cur_tot_cnt > MAX_THREADS_TOTAL - 1 {
                cur_tot_cnt = counters.total_count.condvar.wait(cur_tot_cnt).unwrap();
            }
            *cur_tot_cnt += 1;
        }

        {
            let mut cur_sites_map = counters.sites_count.mutex.lock().unwrap();
            let mut cur_sites_cnt = *cur_sites_map.entry(site.to_string()).or_insert(0);
            while cur_sites_cnt > MAX_THREADS_SITES - 1 {
                cur_sites_map = counters.sites_count.condvar.wait(cur_sites_map).unwrap();
                cur_sites_cnt = *cur_sites_map.entry(site.to_string()).or_insert(0);
            }
            *cur_sites_map.entry(site.to_string()).or_insert(0) += 1;
        }

        let index = Arc::clone(&index);
        let url = url.to_string();
        let title = title.to_string();
        let site = site.to_string();
        let counters2 = Arc::clone(&counters);
        let site2 = site.clone();

        let handle = thread::spawn(move || {
            {
                let article_words = process_article(&article).unwrap();
                index.lock().unwrap().add(
                    site.to_string(),
                    title.to_string(),
                    url.to_string(),
                    article_words,
                );
            }

            {
                let mut cur_tot_cnt = counters2.total_count.mutex.lock().unwrap();
                *cur_tot_cnt -= 1;
                counters2.total_count.condvar.notify_one();
            }

            {
                let mut cur_sites_map = counters2.sites_count.mutex.lock().unwrap();
                *cur_sites_map.entry(site2.to_string()).or_insert(0) -= 1;
                counters2.sites_count.condvar.notify_one();
            }
        });

        handles.push(handle);
    }
    for handle in handles {
        handle.join().unwrap();
    }
    Result::Ok(())
}
