//! Windows のメディアセッションの監視と操作。
//! WinRT のイベントや時間のかかる取得処理は別スレッドで動き、結果は `post_event` でメインスレッドへ渡す。

use windows::core::{AgileReference, Result};
use windows::Foundation::{EventRegistrationToken, TypedEventHandler};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Storage::Streams::{DataReader, IRandomAccessStreamReference};

use crate::debug_log::debug_log;
use crate::imaging::{decode_image, Image};
use crate::win::{post_event, AppEvent};

/// 再生中の曲の情報。
#[derive(Clone)]
pub struct TrackInfo {
    pub title: String,
    pub artist: String,
    /// スレッドをまたいで渡すので AgileReference で包む。
    pub thumbnail: Option<AgileReference<IRandomAccessStreamReference>>,
    pub source_app_id: String,
    /// 再生中か（一時停止・停止中なら false）。
    pub playing: bool,
}

pub enum MediaEvent {
    ManagerReady(std::result::Result<SessionManager, String>),
    SessionChanged,
    PropertiesChanged,
    /// `version` が最新の要求と違う結果は古いので捨てる。
    TrackFetched { version: u64, result: std::result::Result<TrackInfo, String> },
    AlbumArtLoaded { version: u64, result: AlbumArt },
}

pub enum AlbumArt {
    /// 新しい画像。`hash` は元のファイルのバイト列のハッシュ。
    Loaded { image: Image, hash: u64 },
    /// 表示中の画像と同じ内容だった。
    Unchanged,
    /// 読み込めなかった（まだデータが用意されていないことがある）。
    Failed,
}

/// COM を MTA で初期化したワーカースレッドで処理する。
fn spawn_worker(f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        f();
    });
}

fn post(e: MediaEvent) {
    post_event(AppEvent::Media(e));
}

/// セッションマネージャーの取得を始める（完了すると `ManagerReady` が届く）。
pub fn request_manager() {
    spawn_worker(|| {
        let result = SessionManager::RequestAsync().and_then(|op| op.get()).map_err(|e| e.message());
        post(MediaEvent::ManagerReady(result));
    });
}

pub enum UpdateResult {
    /// セッションなし（何も再生していない）。
    NoSession,
    /// 曲情報の取得を始めた（完了すると `TrackFetched` が届く）。
    Fetching,
    /// 再生中でないセッションへの切り替えを保留した。少し待ってから `update` をもう一度呼ぶこと。
    Deferred,
}

/// 監視中のセッションと、そのイベントの登録。
struct Followed {
    session: Session,
    properties_token: EventRegistrationToken,
    playback_token: Option<EventRegistrationToken>,
}

impl Followed {
    fn new(session: &Session) -> Option<Self> {
        let properties_token = session
            .MediaPropertiesChanged(&TypedEventHandler::new(|_, _| {
                post(MediaEvent::PropertiesChanged);
                Ok(())
            }))
            .ok()?;
        // 再生・一時停止が変わったら、どのセッションを表示するか選び直す
        let playback_token = session
            .PlaybackInfoChanged(&TypedEventHandler::new(|_, _| {
                post(MediaEvent::SessionChanged);
                Ok(())
            }))
            .ok();
        Some(Self { session: session.clone(), properties_token, playback_token })
    }
}

impl Drop for Followed {
    fn drop(&mut self) {
        let _ = self.session.RemoveMediaPropertiesChanged(self.properties_token);
        if let Some(token) = self.playback_token {
            let _ = self.session.RemovePlaybackInfoChanged(token);
        }
    }
}

pub struct MediaWatcher {
    manager: SessionManager,
    current_token: EventRegistrationToken,
    sessions_token: Option<EventRegistrationToken>,
    followed: Option<Followed>,
    version: u64,
    /// 再生中でないセッションへの切り替えを1回保留している
    deferred: bool,
}

/// 同じセッションか。WinRT から受け取るオブジェクトは取得のたびに別のものになることがあるので、
/// アプリの ID で比べる（Chromium 系のブラウザはタブが複数あってもセッションは1つ）。
fn same_session(a: Option<&Session>, b: Option<&Session>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a == b || {
                let id = |s: &Session| s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
                let (ia, ib) = (id(a), id(b));
                !ia.is_empty() && ia == ib
            }
        }
        _ => false,
    }
}

/// ログ用: セッションのアプリ名・再生状態・識別子。
fn describe(session: Option<&Session>) -> String {
    let Some(s) = session else { return "-".into() };
    let app = s.SourceAppUserModelId().map(|a| a.to_string()).unwrap_or_default();
    let status = s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).map(|st| st.0).unwrap_or(-1);
    let unknown = windows::core::Interface::cast::<windows::core::IUnknown>(s)
        .map(|u| windows::core::Interface::as_raw(&u) as usize & 0xFFFF)
        .unwrap_or(0);
    format!("{app}#{:x}/u{unknown:x}(st={status})", windows::core::Interface::as_raw(s) as usize & 0xFFFF)
}

fn is_playing(session: &Session) -> bool {
    session
        .GetPlaybackInfo()
        .and_then(|info| info.PlaybackStatus())
        .is_ok_and(|status| status == PlaybackStatus::Playing)
}

impl MediaWatcher {
    pub fn new(manager: SessionManager) -> Result<Self> {
        let current_token = manager.CurrentSessionChanged(&TypedEventHandler::new(|_, _| {
            post(MediaEvent::SessionChanged);
            Ok(())
        }))?;
        let sessions_token = manager
            .SessionsChanged(&TypedEventHandler::new(|_, _| {
                post(MediaEvent::SessionChanged);
                Ok(())
            }))
            .ok();
        Ok(Self { manager, current_token, sessions_token, followed: None, version: 0, deferred: false })
    }

    pub fn current_session(&self) -> Option<&Session> {
        self.followed.as_ref().map(|f| &f.session)
    }

    pub fn is_latest(&self, version: u64) -> bool {
        version == self.version
    }

    /// 表示するセッションを選ぶ。再生中のものを優先し、どれも再生中でなければ今のものを続ける。
    ///
    /// OS の「現在のセッション」は、曲の切り替わりの一瞬に別のタブなどの一時停止中のセッションへ
    /// 移ることがあるので、それだけに頼るとその曲が一瞬表示されてしまう。
    fn choose_session(&self) -> Option<Session> {
        let sessions: Vec<Session> = self.manager.GetSessions().map(|v| v.into_iter().collect()).unwrap_or_default();
        let system = self.manager.GetCurrentSession().ok();
        // 今のセッションがまだあるか（あれば、一覧から取ったオブジェクトを使う）
        let followed = self
            .current_session()
            .and_then(|f| sessions.iter().find(|s| same_session(Some(f), Some(s))))
            .cloned();

        if followed.as_ref().is_some_and(is_playing) {
            return followed;
        }
        if system.as_ref().is_some_and(is_playing) {
            return system;
        }
        if let Some(playing) = sessions.iter().find(|s| is_playing(s)) {
            return Some(playing.clone());
        }
        followed.or(system)
    }

    /// 表示するセッションを選び直し、曲情報の取得を始める。
    /// 2つ目の戻り値はセッションが入れ替わったかどうか。
    pub fn update(&mut self) -> (UpdateResult, bool) {
        self.version += 1;
        let current = self.choose_session();

        let changed = !same_session(self.current_session(), current.as_ref());
        debug_log!(
            "update v{}: followed={} system={} chosen={} changed={} sessions=[{}]",
            self.version,
            describe(self.current_session()),
            describe(self.manager.GetCurrentSession().ok().as_ref()),
            describe(current.as_ref()),
            changed,
            self.manager
                .GetSessions()
                .map(|v| v.into_iter().map(|s| describe(Some(&s))).collect::<Vec<_>>().join(", "))
                .unwrap_or_default(),
        );
        // 再生中のセッションが曲の切り替えで一瞬消える（作り直される）アプリもある。
        // そのとき一時停止中の別のセッションへすぐに移らず、少し待ってから選び直す。
        if changed && self.followed.is_some() && !current.as_ref().is_some_and(is_playing) && !self.deferred {
            self.deferred = true;
            return (UpdateResult::Deferred, false);
        }
        self.deferred = false;
        if changed {
            self.followed = current.as_ref().and_then(Followed::new);
        }

        let Some(session) = current else {
            return (UpdateResult::NoSession, changed);
        };
        let version = self.version;
        spawn_worker(move || {
            let result = fetch_track(&session).map_err(|e| e.message());
            post(MediaEvent::TrackFetched { version, result });
        });
        (UpdateResult::Fetching, changed)
    }

    pub fn play_pause(&self) {
        if let Some(s) = self.current_session() {
            let _ = s.TryTogglePlayPauseAsync();
        }
    }

    pub fn next_track(&self) {
        if let Some(s) = self.current_session() {
            let _ = s.TrySkipNextAsync();
        }
    }

    pub fn previous_track(&self) {
        if let Some(s) = self.current_session() {
            let _ = s.TrySkipPreviousAsync();
        }
    }
}

impl Drop for MediaWatcher {
    fn drop(&mut self) {
        let _ = self.manager.RemoveCurrentSessionChanged(self.current_token);
        if let Some(token) = self.sessions_token {
            let _ = self.manager.RemoveSessionsChanged(token);
        }
        self.followed = None;
    }
}

fn fetch_track(session: &Session) -> Result<TrackInfo> {
    let props = session.TryGetMediaPropertiesAsync()?.get()?;
    Ok(TrackInfo {
        title: props.Title().map(|s| s.to_string()).unwrap_or_default(),
        artist: props.Artist().map(|s| s.to_string()).unwrap_or_default(),
        thumbnail: props.Thumbnail().ok().and_then(|t| AgileReference::new(&t).ok()),
        source_app_id: session.SourceAppUserModelId().map(|s| s.to_string()).unwrap_or_default(),
        playing: is_playing(session),
    })
}

/// サムネイルを別スレッドで読み込み、デコードする（完了すると `AlbumArtLoaded` が届く）。
/// 中身が `current_hash` と同じならデコードせずに `Unchanged` を返す。
pub fn load_album_art(
    thumbnail: AgileReference<IRandomAccessStreamReference>,
    version: u64,
    max_width: u32,
    current_hash: Option<u64>,
) {
    spawn_worker(move || {
        let result = match thumbnail.resolve().and_then(|t| read_stream(&t)) {
            Ok(bytes) if !bytes.is_empty() => {
                let hash = hash_bytes(&bytes);
                if current_hash == Some(hash) {
                    AlbumArt::Unchanged
                } else {
                    match decode_image(&bytes, max_width) {
                        Ok(image) => AlbumArt::Loaded { image, hash },
                        Err(_) => AlbumArt::Failed,
                    }
                }
            }
            _ => AlbumArt::Failed,
        };
        post(MediaEvent::AlbumArtLoaded { version, result });
    });
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn read_stream(reference: &IRandomAccessStreamReference) -> Result<Vec<u8>> {
    let stream = reference.OpenReadAsync()?.get()?;
    let size = stream.Size()?.min(32 * 1024 * 1024) as u32;
    let reader = DataReader::CreateDataReader(&stream)?;
    let loaded = reader.LoadAsync(size)?.get()?;
    let mut bytes = vec![0u8; loaded as usize];
    reader.ReadBytes(&mut bytes)?;
    Ok(bytes)
}
