//! Shared binary-download responder for template/export endpoints.

use std::io::Cursor;

use rocket::http::{ContentType, Status};
use rocket::response::{self, Responder, Response};
use rocket::Request;

/// A streamed byte payload with an explicit MIME type and download filename.
pub struct BinaryDownload {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub filename: String,
}

impl<'r> Responder<'r, 'static> for BinaryDownload {
    fn respond_to(self, _req: &'r Request<'_>) -> response::Result<'static> {
        let content_type = ContentType::parse_flexible(self.mime)
            .unwrap_or(ContentType::Binary);
        // ASCII-strip the filename for the Content-Disposition header.
        let safe_name: String = self
            .filename
            .chars()
            .filter(|c| c.is_ascii() && !c.is_control())
            .collect();
        Response::build()
            .status(Status::Ok)
            .header(content_type)
            .raw_header(
                "Content-Disposition",
                format!("attachment; filename=\"{}\"", safe_name),
            )
            .sized_body(self.bytes.len(), Cursor::new(self.bytes))
            .ok()
    }
}
