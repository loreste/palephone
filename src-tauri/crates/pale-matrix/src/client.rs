use std::path::Path;
use std::time::Duration;

use matrix_sdk::{
    config::SyncSettings,
    ruma::{
        api::client::room::create_room::v3::Request as CreateRoomRequest,
        api::client::typing::create_typing_event::v3::{
            Request as TypingRequest, Typing, TypingInfo,
        },
        events::room::message::{
            MessageType as RumaMessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent,
        },
        events::typing::SyncTypingEvent,
        OwnedUserId, RoomId,
    },
    Client, Room,
};
use tokio::sync::broadcast;

use crate::events::MatrixEvent;
use crate::types::*;

/// The Matrix client wrapper
pub struct MatrixClient {
    client: Option<Client>,
    event_tx: broadcast::Sender<MatrixEvent>,
    data_dir: std::path::PathBuf,
}

impl MatrixClient {
    pub fn new(data_dir: &Path) -> Self {
        let (event_tx, _) = broadcast::channel(512);
        Self {
            client: None,
            event_tx,
            data_dir: data_dir.to_path_buf(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<MatrixEvent> {
        self.event_tx.subscribe()
    }

    pub async fn login(
        &mut self,
        homeserver: &str,
        username: &str,
        password: &str,
    ) -> Result<String, String> {
        self.emit(MatrixEvent::AuthStateChanged {
            state: MatrixAuthState::LoggingIn,
            user_id: None,
            display_name: None,
        });

        let hs_url = if homeserver.starts_with("http") {
            homeserver.to_string()
        } else {
            format!("https://{}", homeserver)
        };

        let store_path = self.data_dir.join("matrix_store");
        std::fs::create_dir_all(&store_path).ok();

        let client = Client::builder()
            .homeserver_url(&hs_url)
            .sqlite_store(&store_path, None)
            .build()
            .await
            .map_err(|e| format!("Failed to create Matrix client: {}", e))?;

        let response = client
            .matrix_auth()
            .login_username(username, password)
            .initial_device_display_name("Pale Desktop")
            .await
            .map_err(|e| format!("Matrix login failed: {}", e))?;

        let user_id = response.user_id.to_string();
        log::info!("Matrix login successful: {}", user_id);

        let display_name = client
            .account()
            .get_display_name()
            .await
            .ok()
            .flatten()
            .map(|n| n.to_string());

        self.client = Some(client);

        self.emit(MatrixEvent::AuthStateChanged {
            state: MatrixAuthState::LoggedIn,
            user_id: Some(user_id.clone()),
            display_name,
        });

        Ok(user_id)
    }

    pub async fn start_sync(&self) -> Result<(), String> {
        let client = self.client.as_ref().ok_or("Not logged in")?.clone();
        let tx = self.event_tx.clone();

        // Register message handler
        let tx_msg = tx.clone();
        client.add_event_handler(move |event: OriginalSyncRoomMessageEvent, room: Room| {
            let tx = tx_msg.clone();
            async move {
                let msg = convert_message(&event, &room).await;
                let _ = tx.send(MatrixEvent::Message(msg));
            }
        });

        let tx_typing = tx.clone();
        let own_user_id = client.user_id().map(|id| id.to_owned());
        client.add_event_handler(move |event: SyncTypingEvent, room: Room| {
            let tx = tx_typing.clone();
            let own_user_id = own_user_id.clone();
            async move {
                let user_ids = event
                    .content
                    .user_ids
                    .into_iter()
                    .filter(|id| own_user_id.as_ref() != Some(id))
                    .map(|id| id.to_string())
                    .collect();
                let _ = tx.send(MatrixEvent::Typing {
                    room_id: room.room_id().to_string(),
                    user_ids,
                });
            }
        });

        let settings = SyncSettings::default();

        // Spawn sync loop on a dedicated thread (matrix-sdk crypto store isn't Send)
        let tx_err = tx.clone();
        let client_for_rooms = client.clone();
        let tx_rooms = tx.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                // Get initial room list
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let rooms = get_room_summaries(&client_for_rooms).await;
                let _ = tx_rooms.send(MatrixEvent::RoomListUpdated { rooms });

                // Start sync
                log::info!("Matrix sync loop started");
                if let Err(e) = client.sync(settings).await {
                    log::error!("Matrix sync error: {}", e);
                    let _ = tx_err.send(MatrixEvent::SyncError {
                        message: e.to_string(),
                    });
                }
            });
        });

        Ok(())
    }

    pub async fn send_message(&self, room_id: &str, body: &str) -> Result<String, String> {
        let client = self.client.as_ref().ok_or("Not logged in")?;
        let room_id = RoomId::parse(room_id).map_err(|e| format!("Invalid room ID: {}", e))?;
        let room = client.get_room(&room_id).ok_or("Room not found")?;
        let content = RoomMessageEventContent::text_plain(body);
        let response = room
            .send(content)
            .await
            .map_err(|e| format!("Failed to send message: {}", e))?;
        Ok(response.response.event_id.to_string())
    }

    pub async fn set_typing(&self, room_id: &str, typing: bool) -> Result<(), String> {
        let client = self.client.as_ref().ok_or("Not logged in")?;
        let user_id = client.user_id().ok_or("Not logged in")?.to_owned();
        let room_id = RoomId::parse(room_id)
            .map_err(|e| format!("Invalid room ID: {}", e))?
            .to_owned();
        let state = if typing {
            Typing::Yes(TypingInfo::new(Duration::from_secs(5)))
        } else {
            Typing::No
        };
        let request = TypingRequest::new(user_id, room_id, state);
        client
            .send(request)
            .await
            .map_err(|e| format!("Failed to send typing state: {}", e))?;
        Ok(())
    }

    pub async fn create_dm(&self, user_id: &str) -> Result<String, String> {
        let client = self.client.as_ref().ok_or("Not logged in")?;
        let user = OwnedUserId::try_from(user_id).map_err(|e| format!("Invalid user ID: {}", e))?;
        let mut request = CreateRoomRequest::new();
        request.is_direct = true;
        request.invite = vec![user];
        let response = client
            .create_room(request)
            .await
            .map_err(|e| format!("Failed to create DM: {}", e))?;
        Ok(response.room_id().to_string())
    }

    pub async fn get_rooms(&self) -> Result<Vec<RoomSummary>, String> {
        let client = self.client.as_ref().ok_or("Not logged in")?;
        Ok(get_room_summaries(client).await)
    }

    pub async fn send_file(&self, room_id: &str, file_path: &str) -> Result<String, String> {
        let client = self.client.as_ref().ok_or("Not logged in")?;
        let room_id = RoomId::parse(room_id).map_err(|e| format!("Invalid room ID: {}", e))?;
        let room = client.get_room(&room_id).ok_or("Room not found")?;

        let file_data = tokio::fs::read(file_path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let filename = std::path::Path::new(file_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());

        let content_type = mime_guess::from_path(file_path)
            .first()
            .unwrap_or(mime::APPLICATION_OCTET_STREAM);

        // Upload the file to the Matrix content repository
        let upload = client
            .media()
            .upload(&content_type, file_data, None)
            .await
            .map_err(|e| format!("Failed to upload file: {}", e))?;

        // Send a file message referencing the uploaded MXC URI
        use matrix_sdk::ruma::events::room::message::FileMessageEventContent;
        let file_content = FileMessageEventContent::plain(filename.clone(), upload.content_uri);
        let content = RoomMessageEventContent::new(RumaMessageType::File(file_content));
        let response = room
            .send(content)
            .await
            .map_err(|e| format!("Failed to send file message: {}", e))?;

        Ok(response.response.event_id.to_string())
    }

    pub async fn logout(&mut self) -> Result<(), String> {
        if let Some(client) = self.client.take() {
            client
                .matrix_auth()
                .logout()
                .await
                .map_err(|e| format!("Logout failed: {}", e))?;
        }
        self.emit(MatrixEvent::AuthStateChanged {
            state: MatrixAuthState::LoggedOut,
            user_id: None,
            display_name: None,
        });
        Ok(())
    }

    pub fn is_logged_in(&self) -> bool {
        self.client.is_some()
    }

    fn emit(&self, event: MatrixEvent) {
        let _ = self.event_tx.send(event);
    }
}

async fn convert_message(event: &OriginalSyncRoomMessageEvent, room: &Room) -> ChatMessage {
    let body = event.content.body().to_string();

    let msg_type = match &event.content.msgtype {
        RumaMessageType::Text(_) => MessageType::Text,
        RumaMessageType::Emote(_) => MessageType::Emote,
        RumaMessageType::Notice(_) => MessageType::Notice,
        RumaMessageType::Image(_) => MessageType::Image {
            url: String::new(),
            thumbnail_url: None,
            width: None,
            height: None,
        },
        RumaMessageType::File(f) => MessageType::File {
            url: String::new(),
            filename: f.filename.clone().unwrap_or_else(|| f.body.clone()),
            size: None,
            mimetype: f.info.as_ref().and_then(|i| i.mimetype.clone()),
        },
        RumaMessageType::Audio(_) => MessageType::Audio {
            url: String::new(),
            duration_ms: None,
        },
        RumaMessageType::Video(_) => MessageType::Video {
            url: String::new(),
            duration_ms: None,
            width: None,
            height: None,
        },
        _ => MessageType::Text,
    };

    let is_own = room
        .client()
        .user_id()
        .map(|uid| *uid == event.sender)
        .unwrap_or(false);

    ChatMessage {
        event_id: event.event_id.to_string(),
        room_id: room.room_id().to_string(),
        sender: event.sender.to_string(),
        sender_name: None,
        body,
        msg_type,
        timestamp: event.origin_server_ts.as_secs().into(),
        is_encrypted: false,
        is_own,
    }
}

async fn get_room_summaries(client: &Client) -> Vec<RoomSummary> {
    let rooms = client.joined_rooms();
    let mut summaries = Vec::with_capacity(rooms.len());

    for room in rooms {
        let name = room
            .display_name()
            .await
            .map(|n| n.to_string())
            .unwrap_or_else(|_| room.room_id().to_string());

        let is_direct = room.is_direct().await.unwrap_or(false);

        summaries.push(RoomSummary {
            room_id: room.room_id().to_string(),
            name,
            is_direct,
            is_encrypted: false,
            last_message: None,
            last_message_sender: None,
            last_message_ts: None,
            unread_count: room
                .unread_notification_counts()
                .notification_count
                .try_into()
                .unwrap_or(0),
            avatar_url: None,
        });
    }

    summaries
}
