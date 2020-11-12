use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use serenity::{
    async_trait,
    client::bridge::gateway::ShardManager,
    http::Http,
    model::prelude::{Activity, ChannelId, GuildId, Ready, ResumedEvent},
    prelude::*,
};
use songbird::{tracks::TrackQueue, Event, EventContext, EventHandler as VoiceEventHandler};
use tracing::{error, info};

pub struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}

pub struct CommandCounter;

impl TypeMapKey for CommandCounter {
    type Value = Arc<RwLock<HashMap<String, u64>>>;
}

pub struct VoiceQueueManager;

impl TypeMapKey for VoiceQueueManager {
    type Value = Arc<Mutex<HashMap<GuildId, TrackQueue>>>;
}

pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        info!(
            "Connected as {}#{} ({})",
            ready.user.name, ready.user.discriminator, ready.user.id
        );
    }

    async fn cache_ready(&self, ctx: Context, guilds: Vec<GuildId>) {
        ctx.set_activity(Activity::playing(
            format!("with {} guilds", guilds.len()).as_str(),
        ))
        .await;
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed");
    }
}

pub struct TrackEndNotifier {
    pub chan_id: ChannelId,
    pub http: Arc<Http>,
}

#[async_trait]
impl VoiceEventHandler for TrackEndNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {
            if let Err(why) = self
                .chan_id
                .say(&self.http, &format!("Tracks ended: {}.", track_list.len()))
                .await
            {
                error!("{}", why);
            }
        }

        None
    }
}

pub struct ChannelDurationNotifier {
    pub chan_id: ChannelId,
    pub count: Arc<AtomicUsize>,
    pub http: Arc<Http>,
}

#[async_trait]
impl VoiceEventHandler for ChannelDurationNotifier {
    async fn act(&self, _: &EventContext<'_>) -> Option<Event> {
        let count_before = self.count.fetch_add(1, Ordering::Relaxed);
        if let Err(why) = self
            .chan_id
            .say(
                &self.http,
                &format!("I've been in this channel for {} minutes", count_before + 1),
            )
            .await
        {
            error!("{}", why);
        }

        None
    }
}
