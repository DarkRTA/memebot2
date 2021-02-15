use rusqlite::Connection;
use serenity::client::Context;
use serenity::framework::standard::macros::{check, command, group};
use serenity::framework::standard::Args;
use serenity::framework::standard::CommandResult;
use serenity::framework::standard::Reason;
use serenity::model::channel::Message;
use std::collections::HashSet;
use std::error::Error;

use crate::misc::IdNameMap;

pub async fn check_perms(ctx: &Context, msg: &Message, mode_str: &str) -> Result<(), Reason> {
    // XXX: this nested function is needed as checkresult does not implement
    // Try. I should probably find a better way to write this at some point
    async fn inner(ctx: &Context, msg: &Message, mode_str: &str) -> Result<bool, Box<dyn Error>> {
        if msg
            .member(ctx)
            .await?
            .permissions(ctx)
            .await?
            .administrator()
        {
            return Ok(true);
        }

        let conn = Connection::open(crate::config::DB_PATH)?;
        let table = sql::table(&conn, *msg.guild_id.unwrap().as_u64())?;

        let mut modes = Modes::new();
        modes.extend(
            &sql::get_perms(&conn, &table, *msg.author.id.as_u64())
                .unwrap_or_default()
                .modes,
        );

        for role in &msg.member.as_ref().unwrap().roles {
            modes.extend(
                &sql::get_perms(&conn, &table, *role.as_u64())
                    .unwrap_or_default()
                    .modes,
            );
        }

        Ok(modes.check(mode_str))
    }

    match inner(ctx, msg, mode_str).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(Reason::User("permission denied".into())),
        Err(x) => Err(Reason::UserAndLog {
            user: "an internal error occured".into(),
            log: format!("error running perms check: {}", x),
        }),
    }
}

#[check]
#[display_in_help(true)]
async fn perms_flag_p(ctx: &Context, msg: &Message) -> Result<(), Reason> {
    check_perms(ctx, msg, "p").await
}

pub struct PermsEntry {
    pub id: u64,
    pub tag: String,
    pub modes: Modes,
}

impl Default for PermsEntry {
    fn default() -> Self {
        Self {
            id: 0,
            tag: String::new(),
            modes: Modes::new(),
        }
    }
}

#[command]
#[aliases(ls)]
#[only_in("guilds")]
#[owner_privilege(true)]
/// Lists all permissions
async fn list(ctx: &Context, msg: &Message) -> CommandResult {
    let conn = Connection::open(crate::config::DB_PATH)?;
    let table = sql::table(&conn, *msg.guild_id.unwrap().as_u64())?;
    let perms = sql::get_all_perms(&conn, &table)?;

    // FIXME: split responses longer than 2k chars
    let mut response = "```\n".to_string();
    for i in perms.iter() {
        response.push_str(&format!("{} {} {}\n", i.id, i.tag, i.modes.to_string()))
    }
    response.push_str("```");
    msg.channel_id.say(&ctx.http, response).await?;
    Ok(())
}

#[command]
#[aliases(add)]
#[num_args(2)]
#[only_in("guilds")]
#[owner_privilege(true)]
#[usage("<id|name> <flags>")]
/// Adds a permission set. The first argument must be in quotes and the second argument is a list
/// of single letter flags.
async fn set(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let conn = Connection::open(crate::config::DB_PATH)?;
    let table = sql::table(&conn, *msg.guild_id.unwrap().as_u64())?;

    let query: String = args.single_quoted()?;
    let mode_str: String = args.single()?;

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
        match sql::set_perms(&conn, &table, id, name, Modes::from_str(&mode_str)) {
            Ok(_) => "permissions set successfully".to_string(),
            Err(_) => "error setting permissions".to_string(),
        }
    });

    msg.channel_id.say(&ctx.http, &res).await?;
    Ok(())
}

#[command]
#[aliases(rm, remove, delete)]
#[num_args(1)]
#[only_in("guilds")]
#[owner_privilege(true)]
#[usage("<id>")]
/// Removes a permission set.
async fn del(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let conn = Connection::open(crate::config::DB_PATH)?;
    let table = sql::table(&conn, *msg.guild_id.unwrap().as_u64())?;

    let query: String = args.single_quoted()?;

    let mut map = IdNameMap::new();
    map.0.extend(
        sql::get_all_perms(&conn, &table)?
            .iter()
            .map(|x| (x.id, x.tag.to_string())),
    );
    let res = map.lookup(&query, |id, _| match sql::del_perms(&conn, &table, id) {
        Ok(_) => "permissions removed successfully".to_string(),
        Err(_) => "error removing permissions".to_string(),
    });

    msg.channel_id.say(&ctx.http, res).await?;
    Ok(())
}

#[group]
#[prefix("perms")]
#[only_in("guilds")]
#[commands(list, set, del)]
#[checks(perms_flag_p)]
#[owner_privilege(true)]
pub struct Permissions;

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Clone, Debug)]
#[repr(transparent)]
pub struct Modes(HashSet<char>);

impl Modes {
    pub fn new() -> Self {
        Self(HashSet::new())
    }
    pub fn from_str(x: &str) -> Self {
        let mut set = HashSet::new();
        set.extend(x.chars());
        Self(set)
    }
    pub fn extend(&mut self, x: &Self) {
        self.0.extend(x.0.iter());
    }
    pub fn check(&self, x: &str) -> bool {
        self.0.iter().filter(|c| x.contains(**c)).count() == x.len()
    }
    pub fn to_string(&self) -> String {
        self.0.iter().collect()
    }
}

// fuck this garbage
mod sql {
    use super::*;
    use rusqlite::{params, Connection, Result};
    use std::str::FromStr;
    pub fn table(conn: &Connection, id: u64) -> Result<String> {
        let table = format!("x{}_perms", id);
        conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS \"{}\" (
                    id CHAR(32) UNIQUE,
                    tag CHAR(32),
                    modes CHAR(32));",
                table
            ),
            params![],
        )?;

        Ok(table)
    }

    pub fn get_all_perms(conn: &Connection, table: &str) -> Result<Vec<PermsEntry>> {
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{}\"", table))?;
        let iter = stmt.query_map(params![], |row| {
            Ok(PermsEntry {
                id: u64::from_str(&row.get::<usize, String>(0)?).unwrap(),
                tag: row.get::<usize, String>(1)?,
                modes: Modes::from_str(&row.get::<usize, String>(2)?),
            })
        })?;

        Ok(iter.filter_map(|i| i.ok()).collect())
    }

    pub fn get_perms(conn: &Connection, table: &str, id: u64) -> Result<PermsEntry> {
        conn.query_row(
            &format!("SELECT * FROM \"{}\" WHERE id=?", table),
            params![id.to_string()],
            |row| {
                Ok(PermsEntry {
                    id: u64::from_str(&row.get::<usize, String>(0)?).unwrap(),
                    tag: row.get::<usize, String>(1)?,
                    modes: Modes::from_str(&row.get::<usize, String>(2)?),
                })
            },
        )
    }

    pub fn set_perms(
        conn: &Connection,
        table: &String,
        id: u64,
        tag: &str,
        mode: Modes,
    ) -> Result<()> {
        conn.execute(
            &format!(
                "INSERT INTO \"{}\" (id, tag, modes) VALUES (?, ?, ?)
                 ON CONFLICT(id) DO UPDATE SET tag=excluded.tag, modes=excluded.modes;",
                table
            ),
            params![id.to_string(), tag, mode.to_string()],
        )
        .unwrap();

        Ok(())
    }

    pub fn del_perms(conn: &Connection, table: &str, id: u64) -> Result<()> {
        conn.execute(
            &format!("DELETE FROM \"{}\" WHERE id=?", table),
            params![id.to_string()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
pub mod test {
    pub mod modes {
        use super::super::*;
        #[test]
        fn from_string() {
            let set: HashSet<char> = ['a', 'b', 'c', 'd'].iter().cloned().collect();
            assert_eq!(Modes(set), Modes::from_str("abcd"));
        }

        #[test]
        fn extend() {
            let mut x = Modes(['a', 'c'].iter().cloned().collect());
            let y = Modes(['b', 'd'].iter().cloned().collect());
            let m = Modes(['a', 'b', 'c', 'd'].iter().cloned().collect());
            x.extend(&y);
            assert_eq!(x, m);
        }

        #[test]
        fn check_single_true() {
            let modes = Modes(['a', 'b', 'c', 'd'].iter().cloned().collect());
            assert!(modes.check("a"));
        }
        #[test]
        fn check_single_false() {
            let modes = Modes(['a', 'b', 'c', 'd'].iter().cloned().collect());
            assert!(!modes.check("e"));
        }
        #[test]
        fn check_multiple_true() {
            let modes = Modes(['a', 'b', 'c', 'd'].iter().cloned().collect());
            assert!(modes.check("ad"));
        }
        #[test]
        fn check_multiple_false() {
            let modes = Modes(['a', 'b', 'c', 'd'].iter().cloned().collect());
            assert!(!modes.check("ae"));
        }
    }
}
