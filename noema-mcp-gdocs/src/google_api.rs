//! Google Docs and Drive API client

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const DRIVE_API_BASE: &str = "https://www.googleapis.com/drive/v3";
const DOCS_API_BASE: &str = "https://docs.googleapis.com/v1";

/// Google Docs API client
pub struct GoogleDocsClient {
    http_client: Client,
    access_token: String,
}

/// File metadata from Drive API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    #[serde(default)]
    pub modified_time: Option<String>,
    #[serde(default)]
    pub created_time: Option<String>,
}

/// Response from Drive API list files
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFileList {
    pub files: Vec<DriveFile>,
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// A tab extracted from a Google Doc
#[derive(Debug, Clone, Serialize)]
pub struct ExtractedTab {
    pub source_tab_id: String,
    pub title: String,
    pub icon: Option<String>,
    pub content_html: String,
    pub parent_tab_id: Option<String>,
    pub tab_index: i32,
}

/// An image extracted from a Google Doc
#[derive(Debug, Clone, Serialize)]
pub struct ExtractedImage {
    pub object_id: String,
    pub data: Vec<u8>,
    pub mime_type: String,
}

/// Result of extracting a Google Doc with tabs and images
#[derive(Debug, Clone, Serialize)]
pub struct ExtractedDocument {
    pub doc_id: String,
    pub title: String,
    pub tabs: Vec<ExtractedTab>,
    /// Maps inline object ID -> image index in images vec
    pub image_mapping: HashMap<String, usize>,
    #[serde(skip_serializing)]
    pub images: Vec<ExtractedImage>,
}

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfo {
    pub id: String,
    pub title: String,
    pub revision_id: Option<String>,
}

// Google Docs API response types
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocsDocument {
    document_id: String,
    title: String,
    revision_id: Option<String>,
    #[serde(default)]
    tabs: Vec<DocsTab>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocsTab {
    tab_properties: TabProperties,
    #[serde(default)]
    document_tab: Option<DocumentTab>,
    #[serde(default)]
    child_tabs: Vec<DocsTab>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TabProperties {
    tab_id: String,
    title: String,
    #[serde(default)]
    parent_tab_id: Option<String>,
    #[serde(default)]
    index: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentTab {
    #[serde(default)]
    inline_objects: HashMap<String, InlineObject>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InlineObject {
    inline_object_properties: InlineObjectProperties,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InlineObjectProperties {
    embedded_object: EmbeddedObject,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmbeddedObject {
    #[serde(default)]
    image_properties: Option<ImageProperties>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImageProperties {
    content_uri: Option<String>,
}

impl GoogleDocsClient {
    /// Create a new client with the given access token
    pub fn new(access_token: String) -> Self {
        Self {
            http_client: Client::new(),
            access_token,
        }
    }

    /// List Google Docs from Drive
    pub async fn list_documents(
        &self,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<DriveFile>> {
        let mut url = format!(
            "{}/files?pageSize={}&fields=files(id,name,mimeType,modifiedTime,createdTime)",
            DRIVE_API_BASE,
            limit.min(100)
        );

        // Filter to only Google Docs
        let mut q = "mimeType='application/vnd.google-apps.document'".to_string();

        // Add user query if provided
        if let Some(user_query) = query {
            q = format!("{} and fullText contains '{}'", q, user_query.replace('\'', "\\'"));
        }

        url = format!("{}&q={}", url, urlencoding::encode(&q));

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Drive API error ({}): {}", status, body));
        }

        let list: DriveFileList = response.json().await?;
        Ok(list.files)
    }

    /// Extract a Google Doc with all tabs and images
    /// Returns raw tab content (as HTML) and image data for noema-core to process
    pub async fn extract_document(&self, doc_id: &str) -> Result<ExtractedDocument> {
        // Fetch document with all tabs content
        let url = format!(
            "{}/documents/{}?includeTabsContent=true",
            DOCS_API_BASE, doc_id
        );

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Docs API error ({}): {}", status, body));
        }

        let doc: DocsDocument = response.json().await?;

        // Collect all inline objects from all tabs
        let mut all_inline_objects: HashMap<String, String> = HashMap::new();
        self.collect_inline_objects(&doc.tabs, &mut all_inline_objects);

        // Fetch all images
        let mut images = Vec::new();
        let mut image_mapping = HashMap::new();

        for (object_id, content_uri) in &all_inline_objects {
            match self.fetch_image(content_uri).await {
                Ok((data, mime_type)) => {
                    image_mapping.insert(object_id.clone(), images.len());
                    images.push(ExtractedImage {
                        object_id: object_id.clone(),
                        data,
                        mime_type,
                    });
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch image {}: {}", object_id, e);
                }
            }
        }

        // Extract tabs as HTML (for noema-core to convert to markdown)
        let tabs = self.extract_tabs_html(doc_id, &doc.tabs, None, &mut 0).await?;

        Ok(ExtractedDocument {
            doc_id: doc.document_id,
            title: doc.title,
            tabs,
            image_mapping,
            images,
        })
    }

    /// Collect all inline object URIs from tabs recursively
    fn collect_inline_objects(
        &self,
        tabs: &[DocsTab],
        objects: &mut HashMap<String, String>,
    ) {
        for tab in tabs {
            if let Some(document_tab) = &tab.document_tab {
                for (object_id, inline_obj) in &document_tab.inline_objects {
                    if let Some(uri) = inline_obj
                        .inline_object_properties
                        .embedded_object
                        .image_properties
                        .as_ref()
                        .and_then(|p| p.content_uri.clone())
                    {
                        objects.insert(object_id.clone(), uri);
                    }
                }
            }
            // Recurse into child tabs
            self.collect_inline_objects(&tab.child_tabs, objects);
        }
    }

    /// Extract tabs as HTML content
    async fn extract_tabs_html(
        &self,
        doc_id: &str,
        tabs: &[DocsTab],
        parent_tab_id: Option<&str>,
        index: &mut i32,
    ) -> Result<Vec<ExtractedTab>> {
        let mut result = Vec::new();

        for tab in tabs {
            let current_index = *index;
            *index += 1;

            // Export this tab as HTML
            let html = self.export_tab_as_html(doc_id, &tab.tab_properties.tab_id).await?;

            result.push(ExtractedTab {
                source_tab_id: tab.tab_properties.tab_id.clone(),
                title: tab.tab_properties.title.clone(),
                icon: None, // Google Docs doesn't have icons in tab properties
                content_html: html,
                parent_tab_id: parent_tab_id.map(String::from),
                tab_index: current_index,
            });

            // Recurse into child tabs
            let children = Box::pin(self.extract_tabs_html(
                doc_id,
                &tab.child_tabs,
                Some(&tab.tab_properties.tab_id),
                index,
            ))
            .await?;
            result.extend(children);
        }

        Ok(result)
    }

    /// Export a specific tab as HTML
    async fn export_tab_as_html(&self, doc_id: &str, tab_id: &str) -> Result<String> {
        let url = format!(
            "{}/files/{}/export?mimeType=text/html&tabId={}",
            DRIVE_API_BASE, doc_id, tab_id
        );

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Drive API export error ({}): {}", status, body));
        }

        Ok(response.text().await?)
    }

    /// Fetch an image from a Google-hosted URL
    async fn fetch_image(&self, url: &str) -> Result<(Vec<u8>, String)> {
        let response = self
            .http_client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(anyhow!("Failed to fetch image ({})", status));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/png")
            .split(';')
            .next()
            .unwrap_or("image/png")
            .to_string();

        let data = response.bytes().await?.to_vec();
        Ok((data, content_type))
    }

    /// Get document content as plain text (simple export, no tabs)
    pub async fn get_document_as_text(&self, doc_id: &str) -> Result<String> {
        let url = format!(
            "{}/files/{}/export?mimeType=text/plain",
            DRIVE_API_BASE, doc_id
        );

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Drive API export error ({}): {}", status, body));
        }

        Ok(response.text().await?)
    }

    /// Get document content as HTML and convert to Markdown (simple export, no tabs)
    pub async fn get_document_as_markdown(&self, doc_id: &str) -> Result<String> {
        let url = format!(
            "{}/files/{}/export?mimeType=text/html",
            DRIVE_API_BASE, doc_id
        );

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Drive API export error ({}): {}", status, body));
        }

        let html = response.text().await?;
        Ok(html2md::parse_html(&html))
    }

    /// Get document metadata
    pub async fn get_document_info(&self, doc_id: &str) -> Result<DocumentInfo> {
        let url = format!("{}/documents/{}", DOCS_API_BASE, doc_id);

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("Docs API error ({}): {}", status, body));
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct DocResponse {
            document_id: String,
            title: String,
            revision_id: Option<String>,
        }

        let doc: DocResponse = response.json().await?;
        Ok(DocumentInfo {
            id: doc.document_id,
            title: doc.title,
            revision_id: doc.revision_id,
        })
    }

    /// Get document revision ID for change detection
    pub async fn get_revision_id(&self, doc_id: &str) -> Result<Option<String>> {
        let info = self.get_document_info(doc_id).await?;
        Ok(info.revision_id)
    }
}

// URL encoding helper
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                    result.push(c);
                }
                _ => {
                    for b in c.to_string().as_bytes() {
                        result.push_str(&format!("%{:02X}", b));
                    }
                }
            }
        }
        result
    }
}
