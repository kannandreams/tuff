use crate::error::Result;

use super::home_dir;

pub fn cmd_cache_clear() -> Result<()> {
    let home = home_dir()?;
    crate::cache::clear(&home)?;
    println!("cleared Coral cache");
    Ok(())
}
