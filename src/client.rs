use futures::stream::Stream;
use futures::{
    StreamExt,
    stream::{self},
};
use reqwest::header::HeaderMap;
use serde::{Serialize, de::DeserializeOwned};
use std::{fmt::Debug, pin::Pin};
use tracing::{Level, event, instrument};

#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,
}

pub type BoxedStream<T> = Pin<Box<dyn Stream<Item = T> + Send>>;

impl Client {
    pub fn default() -> Self {
        Client {
            client: reqwest::Client::new(),
        }
    }

    pub fn with_headers(headers: HeaderMap) -> Self {
        Client {
            client: reqwest::Client::builder()
                .default_headers(headers)
                .build()
                .expect("Failed to build headers"),
        }
    }

    #[instrument(level = "info", skip(self))]
    pub async fn get<U, T>(&self, url: U) -> anyhow::Result<T>
    where
        U: reqwest::IntoUrl + std::fmt::Debug,
        T: DeserializeOwned,
    {
        let response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Request failed with status: {} - {:?}",
                response.status(),
                response.error_for_status()
            ));
        }
        let text = response.text().await?;
        event!(Level::INFO, response = text);

        Ok(serde_json::from_str::<T>(&text)?)
    }

    #[instrument(level = "info", skip(self, request), fields(json_request = serde_json::to_string(request).unwrap()))]
    pub async fn post<U, S, T>(&self, url: U, request: &S) -> anyhow::Result<T>
    where
        U: reqwest::IntoUrl + std::fmt::Debug,
        S: Serialize + Sized,
        T: DeserializeOwned,
    {
        let response = self.client.post(url).json(request).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Request failed with status: {} - {:?}",
                response.status(),
                response.error_for_status()
            ));
        }
        let text = response.text().await?;
        event!(Level::INFO, response = text);

        Ok(serde_json::from_str::<T>(&text)?)
    }

    #[instrument(level = "info", skip(self, request, process), fields(json_request = serde_json::to_string(request).unwrap()))]
    pub async fn post_stream<U, S, F, T>(
        &self,
        url: U,
        request: &S,
        process: F,
    ) -> anyhow::Result<BoxedStream<T>>
    where
        U: reqwest::IntoUrl + Debug,
        S: Serialize + Sized,
        T: DeserializeOwned + Send + 'static,
        F: Fn(&str) -> Option<&str> + 'static + Send,
    {
        let response = self.client.post(url).json(&request).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Request failed with status: {}",
                response.status()
            ));
        }

        let bytes = response.bytes_stream();

        // Use scan to maintain state (buffer) across chunks
        let buffered_stream = bytes.scan(String::new(), move |buffer, chunk| {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error reading chunk: {}", e);
                    return futures::future::ready(Some(vec![]));
                }
            };

            // Append new chunk data to buffer
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete lines (ending with \n)
            let mut messages: Vec<T> = vec![];
            let mut last_newline_pos = 0;

            for (idx, _) in buffer.match_indices('\n') {
                let line = &buffer[last_newline_pos..idx];
                last_newline_pos = idx + 1;

                if let Some(processed) = process(line) {
                    if !processed.trim().is_empty() {
                        match serde_json::from_str::<T>(processed) {
                            Ok(chat_response) => messages.push(chat_response),
                            Err(e) => {
                                eprintln!("Failed to parse line: {}: {}", processed, e);
                            }
                        }
                    }
                }
            }

            // Keep incomplete line in buffer
            *buffer = buffer[last_newline_pos..].to_string();

            futures::future::ready(Some(messages))
        });

        Ok(Box::pin(buffered_stream.flat_map(|messages| stream::iter(messages))))
    }
}
