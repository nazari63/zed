mod participant;

use anyhow::{anyhow, Result};
use client::{call::Call, proto, Client, PeerId, TypedEnvelope};
use collections::HashMap;
use futures::StreamExt;
use gpui::{AsyncAppContext, Entity, ModelContext, ModelHandle, MutableAppContext, Task};
use participant::{LocalParticipant, ParticipantLocation, RemoteParticipant};
use project::Project;
use std::sync::Arc;

pub enum Event {
    PeerChangedActiveProject,
}

pub struct Room {
    id: u64,
    status: RoomStatus,
    local_participant: LocalParticipant,
    remote_participants: HashMap<PeerId, RemoteParticipant>,
    pending_user_ids: Vec<u64>,
    client: Arc<Client>,
    _subscriptions: Vec<client::Subscription>,
}

impl Entity for Room {
    type Event = Event;
}

impl Room {
    fn new(id: u64, client: Arc<Client>, cx: &mut ModelContext<Self>) -> Self {
        let mut client_status = client.status();
        cx.spawn_weak(|this, mut cx| async move {
            let is_connected = client_status
                .next()
                .await
                .map_or(false, |s| s.is_connected());
            // Even if we're initially connected, any future change of the status means we momentarily disconnected.
            if !is_connected || client_status.next().await.is_some() {
                if let Some(this) = this.upgrade(&cx) {
                    let _ = this.update(&mut cx, |this, cx| this.leave(cx));
                }
            }
        })
        .detach();

        Self {
            id,
            status: RoomStatus::Online,
            local_participant: LocalParticipant {
                projects: Default::default(),
            },
            remote_participants: Default::default(),
            pending_user_ids: Default::default(),
            _subscriptions: vec![client.add_message_handler(cx.handle(), Self::handle_room_updated)],
            client,
        }
    }

    pub fn create(
        client: Arc<Client>,
        cx: &mut MutableAppContext,
    ) -> Task<Result<ModelHandle<Self>>> {
        cx.spawn(|mut cx| async move {
            let room = client.request(proto::CreateRoom {}).await?;
            Ok(cx.add_model(|cx| Self::new(room.id, client, cx)))
        })
    }

    pub fn join(
        call: &Call,
        client: Arc<Client>,
        cx: &mut MutableAppContext,
    ) -> Task<Result<ModelHandle<Self>>> {
        let room_id = call.room_id;
        cx.spawn(|mut cx| async move {
            let response = client.request(proto::JoinRoom { id: room_id }).await?;
            let room_proto = response.room.ok_or_else(|| anyhow!("invalid room"))?;
            let room = cx.add_model(|cx| Self::new(room_id, client, cx));
            room.update(&mut cx, |room, cx| room.apply_room_update(room_proto, cx))?;
            Ok(room)
        })
    }

    pub fn leave(&mut self, cx: &mut ModelContext<Self>) -> Result<()> {
        if self.status.is_offline() {
            return Err(anyhow!("room is offline"));
        }

        self.status = RoomStatus::Offline;
        self.remote_participants.clear();
        self.client.send(proto::LeaveRoom { id: self.id })?;
        cx.notify();
        Ok(())
    }

    pub fn remote_participants(&self) -> &HashMap<PeerId, RemoteParticipant> {
        &self.remote_participants
    }

    pub fn pending_user_ids(&self) -> &[u64] {
        &self.pending_user_ids
    }

    async fn handle_room_updated(
        this: ModelHandle<Self>,
        envelope: TypedEnvelope<proto::RoomUpdated>,
        _: Arc<Client>,
        mut cx: AsyncAppContext,
    ) -> Result<()> {
        let room = envelope
            .payload
            .room
            .ok_or_else(|| anyhow!("invalid room"))?;
        this.update(&mut cx, |this, cx| this.apply_room_update(room, cx))?;
        Ok(())
    }

    fn apply_room_update(&mut self, room: proto::Room, cx: &mut ModelContext<Self>) -> Result<()> {
        // TODO: compute diff instead of clearing participants
        self.remote_participants.clear();
        for participant in room.participants {
            if Some(participant.user_id) != self.client.user_id() {
                self.remote_participants.insert(
                    PeerId(participant.peer_id),
                    RemoteParticipant {
                        user_id: participant.user_id,
                        projects: Default::default(), // TODO: populate projects
                        location: ParticipantLocation::from_proto(participant.location)?,
                    },
                );
            }
        }
        self.pending_user_ids = room.pending_user_ids;
        cx.notify();
        Ok(())
    }

    pub fn call(&mut self, to_user_id: u64, cx: &mut ModelContext<Self>) -> Task<Result<()>> {
        if self.status.is_offline() {
            return Task::ready(Err(anyhow!("room is offline")));
        }

        let client = self.client.clone();
        let room_id = self.id;
        cx.foreground().spawn(async move {
            client
                .request(proto::Call {
                    room_id,
                    to_user_id,
                })
                .await?;
            Ok(())
        })
    }

    pub fn publish_project(&mut self, project: ModelHandle<Project>) -> Task<Result<()>> {
        if self.status.is_offline() {
            return Task::ready(Err(anyhow!("room is offline")));
        }

        todo!()
    }

    pub fn unpublish_project(&mut self, project: ModelHandle<Project>) -> Task<Result<()>> {
        if self.status.is_offline() {
            return Task::ready(Err(anyhow!("room is offline")));
        }

        todo!()
    }

    pub fn set_active_project(
        &mut self,
        project: Option<&ModelHandle<Project>>,
    ) -> Task<Result<()>> {
        if self.status.is_offline() {
            return Task::ready(Err(anyhow!("room is offline")));
        }

        todo!()
    }

    pub fn mute(&mut self) -> Task<Result<()>> {
        if self.status.is_offline() {
            return Task::ready(Err(anyhow!("room is offline")));
        }

        todo!()
    }

    pub fn unmute(&mut self) -> Task<Result<()>> {
        if self.status.is_offline() {
            return Task::ready(Err(anyhow!("room is offline")));
        }

        todo!()
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum RoomStatus {
    Online,
    Offline,
}

impl RoomStatus {
    fn is_offline(&self) -> bool {
        matches!(self, RoomStatus::Offline)
    }
}
