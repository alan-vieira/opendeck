use opendeck_db::RekordboxDb;
use rusqlite::Connection;

fn main() {
    let db_path = r"C:\Users\alsvi\AppData\Roaming\Pioneer\rekordbox\master.db";
    let key = "402fd482c38817c35ffa8ffb8c7d93143b749e7d315df7a81732a1ff43608497";
    
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch(&format!("PRAGMA key = '{}';", key)).unwrap();
    
    // Listar todas as tabelas
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'").unwrap();
    let tables: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
    println!("=== TABELAS ===");
    for t in tables {
        println!("{}", t);
    }
    
    // Schema do djmdContent
    println!("\n=== SCHEMA djmdContent ===");
    let mut stmt = conn.prepare("PRAGMA table_info(djmdContent)").unwrap();
    let cols = stmt.query_map([], |row| {
        let cid: i32 = row.get(0)?;
        let name: String = row.get(1)?;
        let ctype: String = row.get(2)?;
        let notnull: i32 = row.get(3)?;
        let dflt: Option<String> = row.get(4)?;
        let pk: i32 = row.get(5)?;
        Ok(format!("  {} {} {} {} {} {}", cid, name, ctype, notnull, dflt.unwrap_or("".to_string()), pk))
    }).unwrap();
    for c in cols {
        println!("{}", c.unwrap());
    }
    
    // Schema do djmdCue
    println!("\n=== SCHEMA djmdCue ===");
    let mut stmt = conn.prepare("PRAGMA table_info(djmdCue)").unwrap();
    let cols = stmt.query_map([], |row| {
        let cid: i32 = row.get(0)?;
        let name: String = row.get(1)?;
        let ctype: String = row.get(2)?;
        let notnull: i32 = row.get(3)?;
        let dflt: Option<String> = row.get(4)?;
        let pk: i32 = row.get(5)?;
        Ok(format!("  {} {} {} {} {} {}", cid, name, ctype, notnull, dflt.unwrap_or("".to_string()), pk))
    }).unwrap();
    for c in cols {
        println!("{}", c.unwrap());
    }
    
    // Sample data
    println!("\n=== AMOSTRA djmdContent (5 linhas) ===");
    let mut stmt = conn.prepare("SELECT * FROM djmdContent LIMIT 5").unwrap();
    let rows = stmt.query_map([], |row| {
        let count = row.as_ref().column_count();
        let mut vals = Vec::new();
        for i in 0..count {
            let val: String = row.get(i).unwrap_or_default();
            vals.push(val);
        }
        Ok(vals.join(" | "))
    }).unwrap();
    for r in rows {
        println!("{}", r.unwrap());
    }
}
