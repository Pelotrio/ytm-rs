//! egui loaders for `https://` thumbnails: the download (on the backend runtime), and
//! the textures made from them, kept under a memory budget.
//!
//! Reusing the backend's reqwest client avoids a second HTTP/TLS stack in the binary.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, ImagePoll, LoadError, SizeHint, SizedTexture, TextureLoadResult, TextureLoader, TexturePoll};
use egui::{TextureHandle, TextureOptions, Vec2};

use crate::backend::Backend;

/// A failed download is tried again when it's next needed after this long.
const RETRY_AFTER: Duration = Duration::from_secs(30);

enum Entry {
    Pending,
    Ready(Arc<[u8]>, Option<String>),
    Failed(String, Instant),
}

fn is_http(uri: &str) -> bool {
    uri.starts_with("https://") || uri.starts_with("http://")
}

pub struct HttpImages {
    backend: Backend,
    cache: Arc<Mutex<HashMap<String, Entry>>>,
}

impl HttpImages {
    pub fn new(backend: Backend) -> Self {
        Self { backend, cache: Default::default() }
    }
}

impl BytesLoader for HttpImages {
    fn id(&self) -> &str {
        egui::generate_loader_id!(HttpImages)
    }

    fn load(&self, _ctx: &egui::Context, uri: &str) -> BytesLoadResult {
        if !is_http(uri) {
            return Err(LoadError::NotSupported);
        }
        let mut cache = self.cache.lock().unwrap();
        match cache.get(uri) {
            Some(Entry::Ready(bytes, mime)) => {
                return Ok(BytesPoll::Ready {
                    size: None,
                    bytes: Bytes::Shared(bytes.clone()),
                    mime: mime.clone(),
                });
            }
            Some(Entry::Pending) => return Ok(BytesPoll::Pending { size: None }),
            Some(Entry::Failed(e, at)) if at.elapsed() < RETRY_AFTER => return Err(LoadError::Loading(e.clone())),
            Some(Entry::Failed(..)) | None => {}
        }
        cache.insert(uri.to_owned(), Entry::Pending);
        drop(cache);

        let (http, cache, uri, backend) =
            (self.backend.http().clone(), self.cache.clone(), uri.to_owned(), self.backend.clone());
        self.backend.spawn(async move {
            let res = async {
                let resp = http.get(&uri).send().await?.error_for_status()?;
                let mime = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                Ok::<_, reqwest::Error>((resp.bytes().await?, mime))
            }
            .await;
            let entry = match res {
                Ok((bytes, mime)) => Entry::Ready(Arc::from(&bytes[..]), mime),
                Err(e) => Entry::Failed(e.to_string(), Instant::now()),
            };
            // Only store if not forgotten while in flight.
            if let Some(slot) = cache.lock().unwrap().get_mut(&uri) {
                *slot = entry;
            }
            backend.wake();
        });
        Ok(BytesPoll::Pending { size: None })
    }

    fn forget(&self, uri: &str) {
        self.cache.lock().unwrap().remove(uri);
    }

    fn forget_all(&self) {
        self.cache.lock().unwrap().clear();
    }

    fn byte_size(&self) -> usize {
        self.cache
            .lock()
            .unwrap()
            .values()
            .map(|e| match e {
                Entry::Ready(b, _) => b.len(),
                _ => 0,
            })
            .sum()
    }

    fn has_pending(&self) -> bool {
        self.cache.lock().unwrap().values().any(|e| matches!(e, Entry::Pending))
    }
}

/// GPU memory the thumbnails may keep before the least recently drawn are dropped.
const TEXTURE_BUDGET: usize = 96 * 1024 * 1024;

struct Texture {
    handle: TextureHandle,
    options: TextureOptions,
    size: Vec2,
    last_used: u64,
}

#[derive(Default)]
struct TextureState {
    pass: u64,
    cache: HashMap<String, Texture>,
}

/// Texture loader for `https://` images. Unlike egui's default one it drops the
/// downloaded and decoded copies once the texture exists (only the GPU copy is
/// needed), and it keeps the textures under [`TEXTURE_BUDGET`] by evicting the ones
/// not drawn for longest, so browsing for hours doesn't grow without bound and nothing
/// on screen is ever thrown away.
#[derive(Default)]
pub struct Textures {
    state: Mutex<TextureState>,
}

impl TextureLoader for Textures {
    fn id(&self) -> &str {
        egui::generate_loader_id!(Textures)
    }

    fn load(&self, ctx: &egui::Context, uri: &str, options: TextureOptions, size_hint: SizeHint) -> TextureLoadResult {
        if !is_http(uri) {
            return Err(LoadError::NotSupported);
        }
        {
            let mut state = self.state.lock().unwrap();
            let pass = state.pass;
            if let Some(t) = state.cache.get_mut(uri).filter(|t| t.options == options) {
                t.last_used = pass;
                return Ok(TexturePoll::Ready { texture: SizedTexture::new(t.handle.id(), t.size) });
            }
        }
        match ctx.try_load_image(uri, size_hint)? {
            ImagePoll::Pending { size } => Ok(TexturePoll::Pending { size }),
            ImagePoll::Ready { image } => {
                let size = image.source_size;
                let handle = ctx.load_texture(uri, image, options);
                let texture = SizedTexture::new(handle.id(), size);
                let loaders = ctx.loaders();
                for l in loaders.bytes.lock().iter() {
                    l.forget(uri);
                }
                for l in loaders.image.lock().iter() {
                    l.forget(uri);
                }
                let mut state = self.state.lock().unwrap();
                let last_used = state.pass;
                state.cache.insert(uri.to_owned(), Texture { handle, options, size, last_used });
                Ok(TexturePoll::Ready { texture })
            }
        }
    }

    fn forget(&self, uri: &str) {
        self.state.lock().unwrap().cache.remove(uri);
    }

    fn forget_all(&self) {
        self.state.lock().unwrap().cache.clear();
    }

    fn end_pass(&self, pass_index: u64) {
        let mut state = self.state.lock().unwrap();
        state.pass = pass_index;
        let mut total = state.cache.values().map(|t| t.handle.byte_size()).sum::<usize>();
        if total <= TEXTURE_BUDGET {
            return;
        }
        // Oldest first; anything drawn in the last couple of frames stays.
        let mut old: Vec<(u64, String)> = state.cache.iter().filter(|(_, t)| t.last_used + 2 < pass_index).map(|(k, t)| (t.last_used, k.clone())).collect();
        old.sort_unstable();
        for (_, uri) in old {
            if total <= TEXTURE_BUDGET * 3 / 4 {
                break;
            }
            if let Some(t) = state.cache.remove(&uri) {
                total -= t.handle.byte_size();
            }
        }
    }

    fn byte_size(&self) -> usize {
        self.state.lock().unwrap().cache.values().map(|t| t.handle.byte_size()).sum()
    }
}
