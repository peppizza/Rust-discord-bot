mod commands;
mod state;

use serenity::{
    client::validate_token,
    framework::{
        standard::{
            macros::{group, hook},
            CommandResult, DispatchError, Reason,
        },
        StandardFramework,
    },
    http::Http,
    model::channel::Message,
    prelude::*,
};

use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
};

use songbird::SerenityInit;

use tokio::{signal, sync::RwLock};
use tracing::{debug, error, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

use commands::music::{join::*, leave::*, mute::*, play::*, skip::*, stop::*, unmute::*};
use commands::{emoji::*, help::*, roles::*, util::*};

use state::*;

#[group]
#[commands(latency, ping, commands)]
struct General;

#[group]
#[commands(new_emoji, remove_emoji, rename_emoji)]
struct Emoji;

#[group]
#[commands(add_role, remove_role, create_role, delete_role)]
struct Role;

#[group]
#[commands(join, leave, mute, play, skip, stop, unmute)]
struct Music;

#[hook]
async fn before(ctx: &Context, msg: &Message, command_name: &str) -> bool {
    debug!(
        "Got command '{}' by user '{}'",
        command_name, msg.author.name
    );

    let counter_lock = {
        let data_read = ctx.data.read().await;

        data_read
            .get::<CommandCounter>()
            .expect("Expected CommandCounter in TypeMap.")
            .clone()
    };

    {
        let mut counter = counter_lock.write().await;

        let entry = counter.entry(command_name.to_string()).or_insert(0);
        *entry += 1;
    }

    true
}

#[hook]
async fn after(_ctx: &Context, _msg: &Message, command_name: &str, command_result: CommandResult) {
    match command_result {
        Ok(()) => debug!("Processed command '{}'", command_name),
        Err(why) => warn!("Command '{}' returned error {}", command_name, why),
    }
}

#[hook]
async fn unknown_command(_ctx: &Context, _msg: &Message, unkown_command_name: &str) {
    debug!("Could not find command named '{}'", unkown_command_name);
}

#[hook]
async fn dispatch_error(ctx: &Context, msg: &Message, error: DispatchError) {
    match error {
        DispatchError::Ratelimited(duration) => {
            let _ = msg
                .channel_id
                .say(
                    &ctx.http,
                    format!("Try this again in {} seconds.", duration.as_secs()),
                )
                .await;
        }
        DispatchError::CheckFailed(check, reason) => {
            if let Reason::User(reason) = reason {
                let _ = msg
                    .channel_id
                    .say(
                        &ctx.http,
                        format!("Check {} failed with error {:?}", check, reason),
                    )
                    .await;
            }
        }

        why => debug!("Command failed with error: {:?}", why),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenv::dotenv()?;

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    let token = env::var("DISCORD_TOKEN")?;

    validate_token(&token)?;

    let http = Http::new_with_token(&token);

    let (owners, _bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            owners.insert(info.owner.id);

            (owners, info.id)
        }
        Err(why) => panic!("Could not access application info: {:?}", why),
    };

    let framework = StandardFramework::new()
        .configure(|c| c.owners(owners).prefix("~"))
        .group(&GENERAL_GROUP)
        .group(&EMOJI_GROUP)
        .group(&ROLE_GROUP)
        .group(&MUSIC_GROUP)
        .before(before)
        .after(after)
        .unrecognised_command(unknown_command)
        .on_dispatch_error(dispatch_error)
        .help(&MY_HELP);

    let mut client = Client::builder(&token)
        .framework(framework)
        .event_handler(Handler)
        .register_songbird()
        .await?;

    {
        let mut data = client.data.write().await;
        data.insert::<CommandCounter>(Arc::new(RwLock::new(HashMap::default())));
        data.insert::<ShardManagerContainer>(client.shard_manager.clone());
        data.insert::<VoiceQueueManager>(Arc::new(Mutex::new(HashMap::new())));
    }

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        shard_manager.lock().await.shutdown_all().await;
    });

    if let Err(why) = client.start_autosharded().await {
        error!("Client error: {:?}", why);
    }

    Ok(())
}
