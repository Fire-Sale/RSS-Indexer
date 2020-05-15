# RSS-Indexer

This is a concurrent RSS-indexer written in Rust that support single-thread, sync/async multi-thread, thread-pool mode. This project is written while taking CS 538 Theory and Design of Programming Languages.



This program will read links from an XML file, read the RSS feeds they pointing at on the Internet. Each of the RSS feeds on the Internet contains a list of news articles. The indexer will download each of these articles and build a word count map that letting you query articles where a given word appears most frequently.



## Usage

`cargo run <filename.xml> [single|multi|async|pool]`

- Sample RSS file: `feeds/small-feed.xml`,  `feeds/medium-feed.xml`.



## Note

Dependencies of this project include `reqwest`, `tokio`, `futures`, `rss`, etc. Although the size of the source code is only 40KB, after compiling the overall size of the project will exceed 700MB. 