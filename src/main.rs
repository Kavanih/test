use reqwest::blocking::Client;
use scraper::{Html, Selector};
use serde::{Serialize, Deserialize};
use serde_json;
use std::{
    fs::File, 
    io::Write, 
    num::NonZeroU32, 
    sync::{Arc, Mutex}, 
    thread, 
};
use tokio::runtime::Runtime;
use governor::{Quota, RateLimiter, state::InMemoryState, clock::QuantaClock, middleware::NoOpMiddleware};
use governor::state::NotKeyed;

#[derive(Debug, Serialize, Deserialize)]
struct Book {
    title: String,
}

fn main() {
    let base_url = "https://books.toscrape.com/";
    let client = Client::new();
    
    // ✅ FIX: Correct initialization of RateLimiter
    let clock = QuantaClock::default();
    let rate_limiter = Arc::new(RateLimiter::<NotKeyed, InMemoryState, QuantaClock, NoOpMiddleware>::new(
        Quota::per_second(NonZeroU32::new(1).unwrap()),
        InMemoryState::default(),
        &clock,
    ));

    let books = Arc::new(Mutex::new(Vec::new()));
    let mut handles = vec![];

    for page_num in 1..=5 {
        let url = if page_num == 1 {
            format!("{}index.html", base_url)
        } else {
            format!("{}catalogue/page-{}.html", base_url, page_num)
        };

        let client = client.clone();
        let books = Arc::clone(&books);
        let rate_limiter = Arc::clone(&rate_limiter);

        let handle = thread::spawn(move || {
            // ✅ FIX: Running async code inside a synchronous function
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                rate_limiter.until_ready().await;
            });

            match client.get(&url).send() {
                Ok(response) => {
                    if let Ok(body) = response.text() {
                        extract_titles(&body, &books);
                    }
                }
                Err(err) => eprintln!("❌ Failed to fetch page {}: {}", page_num, err),
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    save_to_json(&books);
}

fn extract_titles(html: &str, books: &Arc<Mutex<Vec<Book>>>) {
    let document = Html::parse_document(html);
    let selector = Selector::parse("h3 a").unwrap();

    let mut extracted_books = vec![];

    for element in document.select(&selector) {
        if let Some(title) = element.value().attr("title") {
            let title_str = title.to_string();

            // Introduce the intentional error
            if title_str.chars().count() <= 4 {
                eprintln!("❌ Error: Title '{}' has less 5 characters!", title_str);
            } else {
                extracted_books.push(Book { title: title_str });
            }
        }
    }

    let mut books_lock = books.lock().unwrap();
    books_lock.extend(extracted_books);

    println!("✅ Successfully extracted titles");
}

fn save_to_json(books: &Arc<Mutex<Vec<Book>>>) {
    let books_lock = books.lock().unwrap();
    let json_data = serde_json::to_string_pretty(&*books_lock).unwrap();

    let mut file = File::create("titles.json").expect("Failed to create JSON file");
    file.write_all(json_data.as_bytes()).expect("Failed to write JSON file");

    println!("✅ Titles saved to titles.json");
}
