use rusqlite::Connection;
use serenity::client::Context;
use serenity::framework::standard::macros::{check, command, group};
use serenity::framework::standard::Args;
use serenity::framework::standard::CommandResult;
use serenity::framework::standard::Reason;
use serenity::model::channel::Message;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Meme {
    id: i32,
    text: String,
}

// TODO: replace this disgusting string splitting with access to the args object
#[command]
#[only_in("guilds")]
#[usage("[query]")]
/// Gets a meme, optionally searching for one.
///
/// Usage examples:
/// # Getting a random meme:
/// `!meme`
/// # Getting a random meme matching a search:
/// `!meme <search string>`
/// # Getting the latest meme:
/// `!meme 0`
/// # Getting a meme matching an id:
/// `!meme <id number>`
async fn meme(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let conn = Connection::open(crate::config::DB_PATH)?;
    let table = sql::table(&conn, *msg.guild_id.unwrap().as_u64())?;
    let arg = args.rest().to_string();

    let text = if arg.is_empty() {
        sql::random_meme(&conn, &table)?.text
    } else {
        match i32::from_str(&arg) {
            Ok(x) => match if x != 0 {
                sql::meme_by_id(&conn, &table, x)
            } else {
                sql::latest_meme(&conn, &table)
            } {
                Ok(y) => y.text,
                Err(_) => format!("meme {} not found", x),
            },
            Err(_) => match sql::search_meme(&conn, &table, &arg) {
                Ok(y) => y.text,
                Err(_) => format!("meme matching \"{}\" not found", arg),
            },
        }
    };

    msg.channel_id.say(&ctx.http, text).await?;
    Ok(())
}

#[command]
#[only_in("guilds")]
#[checks(edit_memes_check)]
#[usage("<text>")]
/// Adds a meme to the list.
async fn addmeme(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let conn = Connection::open(crate::config::DB_PATH)?;
    let table = sql::table(&conn, *msg.guild_id.unwrap().as_u64())?;
    let arg = args.rest().to_string();

    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    sql::add_meme(&conn, &table, time as i64, &arg)?;
    let id = sql::get_seq(&conn, &table)?;
    msg.channel_id
        .say(&ctx.http, &format!("meme {} added successfully", id))
        .await?;
    Ok(())
}

#[command]
#[only_in("guilds")]
#[checks(edit_memes_check)]
#[usage("<id>")]
/// Removes a meme from the list.
async fn delmeme(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let mut conn = Connection::open(crate::config::DB_PATH)?;
    let table = sql::table(&conn, *msg.guild_id.unwrap().as_u64())?;
    let arg = i32::from_str(args.rest())?;

    // this needs to be in its own scope as tx is not compatable with .await
    let res = {
        let tx = conn.transaction()?;
        let res = match sql::meme_by_id(&tx, &table, arg) {
            Ok(x) => {
                sql::del_meme(&tx, &table, arg)?;
                match sql::latest_meme(&tx, &table) {
                    Ok(x) => sql::set_seq(&tx, &table, x.id),
                    Err(_) => sql::set_seq(&tx, &table, 0),
                }?;
                format!("successfully deleted meme {}: {}", arg, x.text)
            }
            Err(_) => "error deleting meme (it probably doesn't exist to begin with)".into(),
        };
        tx.commit()?;
        res
    };
    msg.channel_id.say(&ctx.http, res).await?;
    Ok(())
}

#[check]
pub async fn edit_memes_check(ctx: &Context, msg: &Message) -> Result<(), Reason> {
    crate::modules::perms::check_perms(ctx, msg, "m").await
}

#[group]
#[only_in("guilds")]
#[commands(meme, addmeme, delmeme)]
pub struct Memes;

mod sql {
    use super::*;
    use rusqlite::{params, Connection, Result};

    pub fn table(conn: &Connection, id: u64) -> Result<String> {
        let table = format!("x{}_memes", id);
        conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS \"{}\" (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    time INT,
                    text VARCHAR(500))",
                table
            ),
            params![],
        )?;

        Ok(table)
    }

    pub fn random_meme(conn: &Connection, table: &str) -> Result<Meme> {
        conn.query_row(
            &format!(
                "SELECT * FROM \"{0}\"
                     LIMIT 1 OFFSET 
                         abs(random()) 
                             % (SELECT count(*) FROM \"{0}\")",
                table,
            ),
            params![],
            |row| {
                Ok(Meme {
                    id: row.get(0)?,
                    text: row.get(2)?,
                })
            },
        )
    }

    pub fn meme_by_id(conn: &Connection, table: &str, id: i32) -> Result<Meme> {
        conn.query_row(
            &format!("SELECT * FROM \"{0}\" WHERE id=?", table),
            params![id],
            |row| {
                Ok(Meme {
                    id: row.get(0)?,
                    text: row.get(2)?,
                })
            },
        )
    }

    pub fn latest_meme(conn: &Connection, table: &str) -> Result<Meme> {
        conn.query_row(
            &format!("SELECT * FROM \"{0}\" ORDER BY id DESC LIMIT 1", table),
            params![],
            |row| {
                Ok(Meme {
                    id: row.get(0)?,
                    text: row.get(2)?,
                })
            },
        )
    }

    pub fn search_meme(conn: &Connection, table: &str, query: &str) -> Result<Meme> {
        conn.query_row(
            &format!(
                "SELECT * FROM \"{0}\" WHERE text LIKE ? ORDER BY random()",
                table
            ),
            params![&format!("%{}%", query)],
            |row| {
                Ok(Meme {
                    id: row.get(0)?,
                    text: row.get(2)?,
                })
            },
        )
    }

    pub fn get_seq(conn: &Connection, table: &str) -> Result<i32> {
        conn.query_row(
            "SELECT seq FROM sqlite_sequence WHERE name=?",
            params![table],
            |row| row.get(0),
        )
    }

    pub fn set_seq(conn: &Connection, table: &str, seq: i32) -> Result<()> {
        conn.execute(
            "UPDATE sqlite_sequence SET seq=? WHERE name=?",
            params![seq, table],
        )?;

        Ok(())
    }

    pub fn del_meme(conn: &Connection, table: &str, id: i32) -> Result<()> {
        conn.execute(
            &format!("DELETE FROM \"{}\" WHERE id=?", table),
            params![id],
        )?;

        Ok(())
    }

    pub fn add_meme(conn: &Connection, table: &str, time: i64, text: &str) -> Result<()> {
        conn.execute(
            &format!("INSERT INTO \"{}\" (time, text) VALUES (?, ?)", table),
            params![time, text],
        )?;

        Ok(())
    }
}
