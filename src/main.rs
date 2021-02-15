#![feature(async_closure)]
mod config;
mod misc;
mod modules;

use serenity::client::{Client, Context, EventHandler};
use serenity::framework::standard::{
    help_commands::plain, macros::help, macros::hook, Args, CommandGroup, CommandResult,
    DispatchError, HelpOptions, Reason, StandardFramework,
};
use serenity::http::Http;
use serenity::model::channel::Message;
use serenity::model::prelude::*;
use std::collections::HashSet;

#[help]
async fn help(
    context: &Context,
    msg: &Message,
    args: Args,
    help_options: &'static HelpOptions,
    groups: &[&'static CommandGroup],
    owners: HashSet<UserId>,
) -> CommandResult {
    plain(context, msg, args, &help_options, groups, owners).await;
    Ok(())
}

struct Handler;
impl EventHandler for Handler {}

#[hook]
async fn dispatch_error(ctx: &Context, msg: &Message, error: DispatchError) {
    match error {
        DispatchError::CheckFailed(_, x) => match x {
            Reason::User(x) => drop(msg.channel_id.say(&ctx.http, &x).await),
            Reason::UserAndLog { user: x, log: _ } => drop(msg.channel_id.say(&ctx.http, &x).await),
            _ => (),
        },
        _ => (),
    };
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let http = Http::new_with_token(config::TOKEN);
    let (owners, bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            if let Some(team) = info.team {
                owners.insert(team.owner_user_id);
            } else {
                owners.insert(info.owner.id);
            }
            match http.get_current_user().await {
                Ok(bot_id) => (owners, bot_id.id),
                Err(why) => panic!("Could not access the bot id: {:?}", why),
            }
        }
        Err(why) => panic!("Could not access application info: {:?}", why),
    };

    let mut client = Client::builder(config::TOKEN)
        .event_handler(Handler)
        .framework(
            StandardFramework::new()
                .configure(|c| c.prefix("!").on_mention(Some(bot_id)).owners(owners))
                .group(&modules::perms::PERMISSIONS_GROUP)
                .group(&modules::memes::MEMES_GROUP)
                .group(&modules::roles::ROLES_GROUP)
                .on_dispatch_error(dispatch_error)
                .help(&HELP),
        )
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }

    Ok(())
}
