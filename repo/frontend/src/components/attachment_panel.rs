//! Reusable attachments panel — lists, uploads and previews binary
//! attachments bound to a parent entity (journal or teaching resource).
//!
//! All mutations flow through the typed [`crate::api::attachments`]
//! wrappers, and the component surfaces the authoritative checksum the
//! backend hands back for both uploads and previews.

use dioxus::prelude::*;
use dioxus_router::prelude::Navigator;
use dioxus_router::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlAnchorElement, HtmlInputElement};

use crate::api::attachments::{self, AttachmentView, ParentType};
use crate::router::AppRoute;
use crate::state::AuthState;

/// Props accepted by [`AttachmentPanel`].
#[derive(Props, Clone, PartialEq)]
pub struct AttachmentPanelProps {
    /// Parent kind (journal or teaching resource).
    pub parent_type: ParentType,
    /// UUID of the parent row.
    pub parent_id: String,
    /// Whether the current user can upload new attachments.
    pub can_upload: bool,
    /// Whether the current user can delete existing attachments.
    pub can_delete: bool,
}

/// Humanises a byte count into a short, unit-suffixed string.
fn humanize_bytes(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes <= 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

/// Returns the first 12 characters of a checksum for compact display,
/// prefixed with `sha256:` when the full string already contains that
/// algorithm tag.
fn short_checksum(full: &str) -> String {
    let core = full
        .strip_prefix("sha256:")
        .unwrap_or(full);
    let trimmed: String = core.chars().take(12).collect();
    format!("sha256:{}…", trimmed)
}

/// Attempts to read the first file currently selected on an
/// `<input type="file">` element referenced by id.
fn read_selected_file(input_id: &str) -> Option<web_sys::File> {
    let doc = web_sys::window()?.document()?;
    let element = doc.get_element_by_id(input_id)?;
    let input: HtmlInputElement = element.dyn_into().ok()?;
    let files = input.files()?;
    files.get(0)
}

/// Opens a URL in a new browser tab by synthesising a hidden anchor and
/// clicking it. This is the least disruptive way to bypass popup
/// blockers when the URL is a `blob:` reference we produced ourselves.
fn open_in_new_tab(url: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(doc) = window.document() else {
        return;
    };
    let Some(anchor) = doc
        .create_element("a")
        .ok()
        .and_then(|e| e.dyn_into::<HtmlAnchorElement>().ok())
    else {
        return;
    };
    anchor.set_href(url);
    anchor.set_target("_blank");
    anchor.set_rel("noopener noreferrer");
    let _ = anchor.click();
}

/// Dispatches a session-expiry redirect to the login page and clears
/// any persisted auth state.
fn handle_session_expired(mut auth: Signal<AuthState>, navigator: Navigator) {
    auth.set(AuthState::default());
    AuthState::clear_storage();
    navigator.push(AppRoute::Login {});
}

/// The reusable attachments panel component.
#[component]
pub fn AttachmentPanel(props: AttachmentPanelProps) -> Element {
    let auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();

    let mut items = use_signal(Vec::<AttachmentView>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let mut banner = use_signal(|| Option::<String>::None);
    let mut selected_category = use_signal(|| "other".to_string());
    let mut uploading = use_signal(|| false);
    let mut preview_note = use_signal(|| Option::<String>::None);

    // Reload trigger — bump this to force a fresh fetch.
    let mut reload_tick = use_signal(|| 0u32);

    // Capture props up front so we can move them into async closures.
    let parent_type = props.parent_type;
    let parent_id = props.parent_id.clone();
    let can_upload = props.can_upload;
    let can_delete = props.can_delete;

    {
        let parent_id_eff = parent_id.clone();
        use_effect(move || {
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => return,
            };
            let parent_id_for_async = parent_id_eff.clone();
            let _tick = reload_tick.read();
            loading.set(true);
            error_msg.set(None);
            spawn(async move {
                match attachments::list_for_parent(&token, parent_type, &parent_id_for_async).await
                {
                    Ok(rows) => {
                        items.set(rows);
                        loading.set(false);
                    }
                    Err(err) => {
                        loading.set(false);
                        if err.is_unauthorized() {
                            handle_session_expired(auth, navigator);
                            return;
                        }
                        error_msg.set(Some(format!(
                            "Failed to load attachments: {}",
                            err.message
                        )));
                    }
                }
            });
        });
    }

    let do_upload = {
        let parent_id = parent_id.clone();
        move |_| {
            if uploading() {
                return;
            }
            let Some(file) = read_selected_file("attachment-file-input") else {
                error_msg.set(Some("Please choose a file to upload.".to_string()));
                return;
            };
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => {
                    handle_session_expired(auth, navigator);
                    return;
                }
            };
            let file_name = file.name();
            let mime = file.type_();
            let mime = if mime.is_empty() {
                "application/octet-stream".to_string()
            } else {
                mime
            };
            let category = selected_category();
            let parent_id_for_async = parent_id.clone();

            // Read the File into a Vec<u8> via FileReader as a promise.
            uploading.set(true);
            error_msg.set(None);
            banner.set(None);
            spawn(async move {
                let bytes = match read_file_bytes(&file).await {
                    Ok(b) => b,
                    Err(msg) => {
                        uploading.set(false);
                        error_msg.set(Some(format!("Failed to read file: {msg}")));
                        return;
                    }
                };
                let result = attachments::upload(
                    &token,
                    parent_type,
                    &parent_id_for_async,
                    &file_name,
                    &mime,
                    bytes,
                    Some(&category),
                )
                .await;
                uploading.set(false);
                match result {
                    Ok(view) => {
                        banner.set(Some(format!(
                            "Upload complete — sha256:{} ({} bytes)",
                            view.sha256_checksum
                                .strip_prefix("sha256:")
                                .unwrap_or(&view.sha256_checksum),
                            view.size_bytes
                        )));
                        reload_tick.with_mut(|n| *n += 1);
                    }
                    Err(err) => {
                        if err.is_unauthorized() {
                            handle_session_expired(auth, navigator);
                            return;
                        }
                        error_msg.set(Some(err.message));
                    }
                }
            });
        }
    };

    rsx! {
        section { class: "attachment-panel",
            header { class: "attachment-panel__header",
                h2 { "Attachments" }
                p { class: "text-muted", "Binary files attached to this record." }
            }

            if let Some(b) = banner() {
                div { class: "save-banner", "{b}" }
            }
            if let Some(e) = error_msg() {
                div { class: "error-banner", "{e}" }
            }
            if let Some(n) = preview_note() {
                div { class: "save-banner", "{n}" }
            }

            if can_upload {
                div { class: "upload-form",
                    input {
                        id: "attachment-file-input",
                        r#type: "file",
                        disabled: uploading(),
                    }
                    select {
                        value: "{selected_category()}",
                        oninput: move |evt| selected_category.set(evt.value()),
                        disabled: uploading(),
                        option { value: "sample_issue", "sample_issue" }
                        option { value: "contract", "contract" }
                        option { value: "vendor_quote", "vendor_quote" }
                        option { value: "other", "other" }
                    }
                    button {
                        r#type: "button",
                        disabled: uploading(),
                        onclick: do_upload,
                        if uploading() { "Uploading..." } else { "Upload" }
                    }
                }
            }

            if loading() {
                p { class: "text-muted", "Loading attachments..." }
            } else if items.read().is_empty() {
                p { class: "text-muted", "No attachments yet." }
            } else {
                div { class: "attachment-table",
                    div { class: "attachment-row attachment-row--head",
                        div { "File" }
                        div { "Category" }
                        div { "Size" }
                        div { "Type" }
                        div { "Checksum" }
                        div { "Uploaded" }
                        div { "Actions" }
                    }
                    for att in items.read().iter().cloned() {
                        AttachmentRow {
                            key: "{att.id}",
                            attachment: att.clone(),
                            can_delete: can_delete,
                            on_changed: move |_| reload_tick.with_mut(|n| *n += 1),
                            on_preview_note: move |msg: String| preview_note.set(Some(msg)),
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct AttachmentRowProps {
    attachment: AttachmentView,
    can_delete: bool,
    on_changed: EventHandler<()>,
    on_preview_note: EventHandler<String>,
}

#[component]
fn AttachmentRow(props: AttachmentRowProps) -> Element {
    let auth = use_context::<Signal<AuthState>>();
    let navigator = use_navigator();
    let mut busy = use_signal(|| false);

    let att = props.attachment.clone();
    let att_id = att.id.clone();
    let can_delete = props.can_delete;
    let is_previewable = att.is_previewable;
    let on_changed = props.on_changed;
    let on_preview_note = props.on_preview_note;

    let short = short_checksum(&att.sha256_checksum);
    let full_checksum = att.sha256_checksum.clone();
    let size_display = humanize_bytes(att.size_bytes);
    let category_display = att.category.clone().unwrap_or_else(|| "-".to_string());

    let do_preview = {
        let att_id = att_id.clone();
        move |_| {
            if busy() {
                return;
            }
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => {
                    handle_session_expired(auth, navigator);
                    return;
                }
            };
            let id = att_id.clone();
            busy.set(true);
            spawn(async move {
                match attachments::fetch_preview_blob_url(&token, &id).await {
                    Ok((url, checksum)) => {
                        busy.set(false);
                        open_in_new_tab(&url);
                        on_preview_note.call(format!(
                            "Checksum verified — {}",
                            checksum
                        ));
                    }
                    Err(err) => {
                        busy.set(false);
                        if err.is_unauthorized() {
                            handle_session_expired(auth, navigator);
                            return;
                        }
                        on_preview_note
                            .call(format!("Preview failed: {}", err.message));
                    }
                }
            });
        }
    };

    let do_delete = {
        let att_id = att_id.clone();
        let file_label = att.original_filename.clone();
        move |_| {
            if busy() {
                return;
            }
            let Some(window) = web_sys::window() else {
                return;
            };
            let confirmed = window
                .confirm_with_message(&format!("Delete {}?", file_label))
                .unwrap_or(false);
            if !confirmed {
                return;
            }
            let token = match auth.read().token.clone() {
                Some(t) => t,
                None => {
                    handle_session_expired(auth, navigator);
                    return;
                }
            };
            let id = att_id.clone();
            busy.set(true);
            spawn(async move {
                match attachments::delete(&token, &id).await {
                    Ok(()) => {
                        busy.set(false);
                        on_changed.call(());
                    }
                    Err(err) => {
                        busy.set(false);
                        if err.is_unauthorized() {
                            handle_session_expired(auth, navigator);
                            return;
                        }
                        on_preview_note
                            .call(format!("Delete failed: {}", err.message));
                    }
                }
            });
        }
    };

    let copy_checksum = {
        let full = full_checksum.clone();
        move |_| {
            // Use navigator.clipboard.writeText via JS reflection so we
            // don't need the web-sys Clipboard feature flag.
            let Some(window) = web_sys::window() else {
                return;
            };
            let navigator = window.navigator();
            let nav_js: &wasm_bindgen::JsValue = navigator.as_ref();
            if let Ok(clipboard) =
                js_sys::Reflect::get(nav_js, &wasm_bindgen::JsValue::from_str("clipboard"))
            {
                if !clipboard.is_undefined() && !clipboard.is_null() {
                    if let Ok(write_text) = js_sys::Reflect::get(
                        &clipboard,
                        &wasm_bindgen::JsValue::from_str("writeText"),
                    ) {
                        if let Ok(func) = write_text.dyn_into::<js_sys::Function>() {
                            let _ = func.call1(
                                &clipboard,
                                &wasm_bindgen::JsValue::from_str(&full),
                            );
                        }
                    }
                }
            }
        }
    };

    rsx! {
        div { class: "attachment-row",
            div { class: "attachment-filename", "{att.original_filename}" }
            div { "{category_display}" }
            div { "{size_display}" }
            div { class: "text-muted", "{att.mime_type}" }
            div { class: "checksum-cell",
                span { title: "{full_checksum}", "{short}" }
                button {
                    r#type: "button",
                    class: "link-button",
                    onclick: copy_checksum,
                    "copy"
                }
            }
            div { class: "text-muted", "{att.created_at}" }
            div { class: "attachment-actions",
                if is_previewable {
                    button {
                        r#type: "button",
                        disabled: busy(),
                        onclick: do_preview,
                        if busy() { "..." } else { "Preview" }
                    }
                }
                if can_delete {
                    button {
                        r#type: "button",
                        class: "danger-button",
                        disabled: busy(),
                        onclick: do_delete,
                        "Delete"
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// File -> Vec<u8> — reads a browser File via FileReader and resolves the
// returned promise to the underlying bytes.
// ---------------------------------------------------------------------------

async fn read_file_bytes(file: &web_sys::File) -> Result<Vec<u8>, String> {
    use js_sys::Uint8Array;
    use wasm_bindgen_futures::JsFuture;

    // File extends Blob, so `.array_buffer()` is available and returns
    // a Promise<ArrayBuffer>.
    let promise = file.array_buffer();
    let buffer_js = JsFuture::from(promise)
        .await
        .map_err(|e| format!("FileReader failed: {:?}", e))?;
    let buffer: js_sys::ArrayBuffer = buffer_js
        .dyn_into()
        .map_err(|_| "ArrayBuffer cast failed".to_string())?;
    let array = Uint8Array::new(&buffer);
    let mut bytes = vec![0u8; array.length() as usize];
    array.copy_to(&mut bytes);
    Ok(bytes)
}

