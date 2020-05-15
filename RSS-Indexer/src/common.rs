use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use scraper::{Html, Selector};

pub type RssIndexResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug)]
pub enum RssIndexError {
    ArgsError,
    UrlError,
    ScraperError,
}

impl fmt::Display for RssIndexError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RssIndexError::ArgsError => write!(f, "ArgsError"),
            RssIndexError::UrlError => write!(f, "UrlError"),
            RssIndexError::ScraperError => write!(f, "ScraperError"),
        }
    }
}

impl Error for RssIndexError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

/// Bag of words
#[derive(Default)]
pub struct Bag<T>
where
    T: Ord,
{
    pub counts: BTreeMap<T, u32>,
}

/// The URL and title of an article.
#[derive(PartialEq, Eq, Hash, Clone)]
pub struct Article {
    pub url: String,
    pub title: String,
}

/// The site (hostname) and title of an article.
#[derive(PartialEq, Eq, Hash, Clone)]
pub struct ArticleKey {
    pub site: String,
    pub title: String,
}

/// Map from an ArticleKey to a vector of URLs and bag of words appearing in this article.
pub struct ArticleIndex {
    pub index: HashMap<ArticleKey, (Vec<String>, Bag<String>)>,
}

/// Map from words to Maps from Articles to counts of how frequently the word shows up in the
/// article. Articles not containing this word should not be in the inner HashMap.
pub struct RssIndex {
    pub index: HashMap<String, HashMap<Article, u32>>,
}

impl<T> Bag<T>
where
    T: Ord,
{
    pub fn new() -> Self {
        Bag {
            counts: BTreeMap::new(),
        }
    }

    /// Add one element.
    pub fn add(&mut self, t: T) {
        *self.counts.entry(t).or_insert(0) += 1;
    }

    /// Take intersection with another Bag. The count of any element appearing in both Bags should
    /// be the minimum of the counts in each Bag. Elements appearing in just one bag should have
    /// count zero.
    pub fn intersect_with(&mut self, other: &Self) {
        let mut my_iter = self.counts.iter_mut();
        let mut other_iter = other.counts.iter();

        let mut my_cur = my_iter.next();
        let mut other_cur = other_iter.next();

        while let (Some(my_kv), Some(ot_kv)) = (&mut my_cur, &other_cur) {
            match my_kv.0.cmp(&ot_kv.0) {
                Ordering::Less => {
                    *my_kv.1 = 0;
                    my_cur = my_iter.next();
                }
                Ordering::Greater => {
                    other_cur = other_iter.next();
                }
                Ordering::Equal => {
                    *my_kv.1 = (*my_kv.1).min(*ot_kv.1);
                    my_cur = my_iter.next();
                    other_cur = other_iter.next();
                }
            }
        }
    }
}

impl Article {
    pub fn new(url: String, title: String) -> Self {
        Article { url, title }
    }

    pub fn cmp_title(&self, other: &Article) -> std::cmp::Ordering {
        self.title.cmp(&other.title)
    }
}

impl ArticleKey {
    pub fn new(site: String, title: String) -> Self {
        ArticleKey { site, title }
    }
}

impl ArticleIndex {
    pub fn new() -> Self {
        ArticleIndex {
            index: HashMap::new(),
        }
    }

    /// Add a site, title, URL, and a bag of words to the ArticleIndex.
    pub fn add(&mut self, site: String, title: String, url: String, words: Bag<String>) {
        let key = ArticleKey::new(site, title);
        self.index
            .entry(key)
            .and_modify(|e| {
                e.0.push(url.clone());
                e.1.intersect_with(&words)
            })
            .or_insert((vec![url], words));
    }
}

impl RssIndex {
    pub fn new() -> Self {
        RssIndex {
            index: HashMap::new(),
        }
    }

    /// Add a article, word, and count to the RssIndex. The word might not be in the index.
    pub fn add(&mut self, article: Article, word: String, freq: u32) {
        *self
            .index
            .entry(word)
            .or_insert_with(HashMap::new)
            .entry(article)
            .or_insert(0) += freq;
    }
}

/// Turn an ArticleIndex into an RssIndex.
///
/// If an article has multiple URLs in the ArticleIndex, sort the URLs and file the words under the
/// alphabetically-earliest URL with RssIndex::add here. If an entry in ArticleIndex doesn't have a
/// URL for some reason, just skip it. This function should not panic.
pub fn build_index(article_index: &mut ArticleIndex, rss_index: &mut RssIndex) {
    println!("Building index...");
    for (article_key, article_entry) in &mut article_index.index {
        article_entry.0.sort();
        if let Some(url) = article_entry.0.pop() {
            for (word, count) in &article_entry.1.counts {
                let article = Article::new(url.clone(), article_key.title.clone());
                rss_index.add(article, word.to_string(), *count)
            }
        }
    }
}

/// Delimiters for splitting a string
const DELIMS: &str = " \t\r\n!@#$%^&*()_-+=~`{[}]|\\\"':;<,>.?/";

/// Use reqwest to fetch the article URL, use scraper to parse the document, select the "body" tag,
/// and get the text, split each string on DELIMS (see `std::string::split` and
/// `std::string::contains`), convert each piece to lowercase, and return a vector of words
/// appearing in the article (possibly with duplicates).
pub fn process_article(article: &Article) -> RssIndexResult<Bag<String>> {
    let mut words = Bag::new();
    let content = reqwest::blocking::get(&article.url)?.text()?;
    let parsed = Html::parse_document(&content);
    let body_selector = Selector::parse("body").map_err(|_| RssIndexError::ScraperError)?;
    let body = parsed
        .select(&body_selector)
        .next()
        .ok_or(RssIndexError::ScraperError)?;

    for text in body.text() {
        for tok in text.split(|c| DELIMS.contains(c)) {
            if !tok.is_empty() {
                words.add(tok.to_string().to_lowercase());
            }
        }
    }
    Result::Ok(words)
}
