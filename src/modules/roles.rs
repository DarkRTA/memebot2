use rusqlite::Connection;
use serenity::client::Context;
use serenity::framework::standard::macros::{check, command, group};
use serenity::framework::standard::Args;
use serenity::framework::standard::CommandResult;
use serenity::framework::standard::Reason;
use serenity::model::channel::Message;

use crate::misc::IdNameMap;

#[check]
#[display_in_help(true)]
async fn roles_flag_r(ctx: &Context, msg: &Message) -> Result<(), Reason> {
    crate::modules::perms::check_perms(ctx, msg, "r").await
}

pub struct RolesEntry {
    pub id: u64,
    pub tag: String,
}

impl Default for RolesEntry {
    fn default() -> Self {
        Self {
            id: 0,
            tag: String::new(),
        }
    }
}

#[command]
#[aliases(ls)]
#[only_in("guilds")]
/// Lists all self-assignable roles
async fn list(ctx: &Context, msg: &Message) -> CommandResult {
    let conn = Connection::open(crate::config::DB_PATH)?;
    let table = sql::table(&conn, *msg.guild_id.unwrap().as_u64())?;
    let roles = sql::get_all_roles(&conn, &table)?;

    // FIXME: split responses longer than 2k chars
    let mut response = "Available Roles: ```\n".to_string();
    for i in roles.iter() {
        response.push_str(&format!("{} {}\n", i.id, i.tag))
    }
    response.push_str("```");
    msg.channel_id.say(&ctx.http, response).await?;
    Ok(())
}

#[command]
#[checks(roles_flag_r)]
#[only_in("guilds")]
#[usage("<id|name>")]
/// Adds a self-assignable role
async fn add(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let conn = Connection::open(crate::config::DB_PATH)?;
    let table = sql::table(&conn, *msg.guild_id.unwrap().as_u64())?;

    let query = args.rest().to_string();

    let mut map = IdNameMap::new();

    map.0.extend(
        msg.guild(ctx)
            .await
            .ok_or("None")?
            .roles
            .iter()
            .map(|(k, v)| (*k.as_u64(), v.name.clone())),
    );

    let res = map.lookup(&query, |id, name| {
        match sql::add_role(&conn, &table, id, name) {
            Ok(_) => "self-assignable role added sucessfully".to_string(),
            Err(_) => "error adding self-assignable role".to_string(),
        }
    });

    msg.channel_id.say(&ctx.http, &res).await?;
    Ok(())
}

#[command]
#[aliases(rm, remove, delete)]
#[checks(roles_flag_r)]
#[only_in("guilds")]
#[usage("<id|name>")]
/// Removes a self-assignable role
async fn del(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let conn = Connection::open(crate::config::DB_PATH)?;
    let table = sql::table(&conn, *msg.guild_id.unwrap().as_u64())?;

    let query = args.rest().to_string();
    let mut map = IdNameMap::new();
    map.0.extend(
        sql::get_all_roles(&conn, &table)?
            .iter()
            .map(|x| (x.id, x.tag.to_string())),
    );
    let res = map.lookup(&query, |id, _| match sql::del_role(&conn, &table, id) {
        Ok(_) => "self-assignable role removed successfully".to_string(),
        Err(_) => "error removing self-assignable role".to_string(),
    });

    msg.channel_id.say(&ctx.http, res).await?;
    Ok(())
}

#[command]
#[only_in("guilds")]
#[usage("<id|name>")]
/// Adds or removes the given role from you. Only roles that have been explicity
/// added to the bot may be toggled.
async fn toggle(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let conn = Connection::open(crate::config::DB_PATH)?;
    let table = sql::table(&conn, *msg.guild_id.unwrap().as_u64())?;

    let arg = args.rest().to_string();

    let mut map = IdNameMap::new();
    map.0.extend(
        sql::get_all_roles(&conn, &table)?
            .iter()
            .map(|x| (x.id, x.tag.to_string())),
    );

    // fuck you async
    let http = ctx.http.clone();
    let mut member = msg.member(ctx).await?;
    let res = map.lookup(&arg, move |id, _| {
        tokio::spawn(async move {
            match member.roles.iter().filter(|x| *x.as_u64() == id).count() {
                0 => drop(member.add_role(http, id).await),
                1 => drop(member.remove_role(http, id).await),
                _ => unreachable!(),
            }
        });
        "role toggled successfully".to_string()
    });

    msg.channel_id.say(&ctx.http, res).await?;
    Ok(())
}

#[group]
#[prefix("roles")]
#[only_in("guilds")]
#[commands(list, add, del, toggle)]
/// The roles group contains commands for managing self-assignable roles.
///
/// `!roles list` - lists all self-assignable roles
/// `!roles toggle <id|name>` - gives/revokes a role from you
///
/// The following commands require the `r` permission flag.
///
/// `!roles add <id|name>` - marks a role as self-assignable
/// `!roles del <id|name>` - unmarks a role as self-assignable
pub struct Roles;

mod sql {
    use super::*;
    use rusqlite::{params, Connection, Result};
    use std::str::FromStr;
    pub fn table(conn: &Connection, id: u64) -> Result<String> {
        let table = format!("x{}_roles", id);
        conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS \"{}\" (
                    id CHAR(32) UNIQUE,
                    tag CHAR(32));",
                table
            ),
            params![],
        )?;

        Ok(table)
    }

    pub fn get_all_roles(conn: &Connection, table: &str) -> Result<Vec<RolesEntry>> {
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{}\"", table))?;
        let iter = stmt.query_map(params![], |row| {
            Ok(RolesEntry {
                id: u64::from_str(&row.get::<usize, String>(0)?).unwrap(),
                tag: row.get::<usize, String>(1)?,
            })
        })?;

        Ok(iter.filter_map(|i| i.ok()).collect())
    }

    pub fn add_role(conn: &Connection, table: &String, id: u64, tag: &str) -> Result<()> {
        conn.execute(
            &format!(
                "INSERT INTO \"{}\" (id, tag) VALUES (?, ?)
                 ON CONFLICT(id) DO UPDATE SET tag=excluded.tag",
                table
            ),
            params![id.to_string(), tag],
        )
        .unwrap();

        Ok(())
    }

    pub fn del_role(conn: &Connection, table: &str, id: u64) -> Result<()> {
        conn.execute(
            &format!("DELETE FROM \"{}\" WHERE id=?", table),
            params![id.to_string()],
        )?;
        Ok(())
    }
}
