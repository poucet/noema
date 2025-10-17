
use reqwest::header::HeaderMap;
use serde::{de::DeserializeOwned, Serialize};
use futures::stream::Stream;
use std::pin::Pin;
use futures::{stream::{self}, StreamExt};

#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,
}

pub type BoxedStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;


impl Client {
    pub fn default() -> Self {
        Client {
            client: reqwest::Client::new()
        }
    }

    pub fn with_headers(headers: HeaderMap) -> Self {
        Client {
            client: reqwest::Client::builder().default_headers(headers).build().unwrap()
        }
    }

    pub async fn get<U, T>(&self, url: U) -> anyhow::Result<T>
    where
        U: reqwest::IntoUrl,
        T: DeserializeOwned,
    {
        let response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Request failed with status: {}", response.status()))
        }
        Ok(response.json::<T>().await?)
    }

    pub async fn post<U, S, T>(&self, url: U, request: &S) -> anyhow::Result<T>
    where 
        U: reqwest::IntoUrl,
        S: Serialize + Sized,
        T: DeserializeOwned,
    {
        let response = self.client.post(url).json(request).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Request failed with status: {}", response.status()));
        }

        Ok(response.json::<T>().await?)
    }

    pub async fn post_stream<U, S, F, T>(&self, url: U, request: S, process: F) -> anyhow::Result<BoxedStream<T>>
    where 
        U: reqwest::IntoUrl,
        S: Serialize + Sized,
        T: DeserializeOwned + Send + 'static,
        F: Fn(&str) -> Option<&str> + 'static + Send,
    {
        let response = self.client.post(url).json(&request).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Request failed with status: {}", response.status()));
        }

        let bytes = response.bytes_stream();
        Ok(Box::pin(bytes.flat_map(move |chunk| {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error reading chunk: {}", e);
                    return stream::iter(vec![]);
                }
            };
            let chunk_str = String::from_utf8_lossy(&chunk);
            let messages: Vec<T> = chunk_str
                .lines()
                .filter_map(|line| process(line))
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| {
                    match serde_json::from_str::<T>(line) {
                        Ok(chat_response) => Some(chat_response),
                        Err(e) => {
                            eprintln!("Failed to parse chunk: {}: {}", line, e);
                            None
                        }
                    }
                })
                .collect();
            stream::iter(messages)
        })))
    }
}