use sea_orm::{ConnectionTrait, DatabaseConnection};

use crate::error::AppError;

/// Migrate cache keys to include the badge shape and background suffixes.
///
/// Before this change, cache suffixes ended (per image type) with the badge
/// size token followed by the image size, e.g.:
///   `…{badge_size}.z{image_size}`
/// After this change they include shape (`.shr`) and background (`.bgd`)
/// immediately after the badge size:
///   `…{badge_size}.shr.bgd.z{image_size}`
///
/// All existing cached images were rendered before these settings existed, so
/// they correspond to the defaults — rounded shape (`r`) and default background
/// (`d`). We insert `.shr.bgd` right after the badge size token (`.bxs`, `.bs`,
/// `.bm`, `.bl`, `.bxl`), which is the single uniform anchor across posters,
/// logos, backdrops, and episodes (shape/background always sit immediately
/// after the badge size, before the optional split/blur tokens).
pub async fn run(
    db: &DatabaseConnection,
    cache_dir: &str,
    external_cache_only: bool,
) -> Result<(), AppError> {
    run_db(db).await?;
    run_fs(cache_dir, external_cache_only).await?;
    Ok(())
}

/// The five badge size tokens, each surrounded by dots so it only matches the
/// size suffix (not a like-named substring of another token).
const SIZE_TOKENS: [&str; 5] = [".bxs.", ".bs.", ".bm.", ".bl.", ".bxl."];

/// Default shape (`r`) + background (`d`) suffixes inserted for pre-existing
/// images, which were all rendered with the original look.
const DEFAULT_SUFFIX: &str = "shr.bgd.";

/// Database step — insert `.shr.bgd` after the badge size token in every cache
/// key that doesn't already carry a background suffix (`.bg`).
pub async fn run_db(db: &impl ConnectionTrait) -> Result<(), AppError> {
    let mut total = 0u64;

    // A key has exactly one badge size token, so at most one of these matches.
    // The `instr(cache_key, '.bg') = 0` guard makes the migration idempotent —
    // once `.bgd` is present the row is skipped.
    for token in SIZE_TOKENS {
        let new = format!("{token}{DEFAULT_SUFFIX}"); // e.g. ".bm." -> ".bm.shr.bgd."
        let old_len = token.len() as i32;
        let result = db
            .execute(sea_orm::Statement::from_string(
                sea_orm::DatabaseBackend::Sqlite,
                format!(
                    "UPDATE image_meta SET cache_key = \
                     substr(cache_key, 1, instr(cache_key, '{token}') - 1) || '{new}' || \
                     substr(cache_key, instr(cache_key, '{token}') + {old_len}) \
                     WHERE instr(cache_key, '{token}') > 0 AND instr(cache_key, '.bg') = 0"
                ),
            ))
            .await?;
        total += result.rows_affected();
    }

    tracing::info!(db_rows = total, "cache keys migrated (added shape/background suffixes)");
    Ok(())
}

/// Filesystem step — rename cached image files to include the shape/background
/// suffixes, for every rendered image type (posters, logos, backdrops,
/// episodes). Base images and previews are unaffected (no badge suffixes).
pub async fn run_fs(cache_dir: &str, external_cache_only: bool) -> Result<(), AppError> {
    if external_cache_only {
        tracing::info!("shape/background filesystem rename skipped (external_cache_only)");
        return Ok(());
    }

    let cache_dir = cache_dir.to_string();
    let renamed = tokio::task::spawn_blocking(move || {
        let mut count = 0u64;
        for subdir in ["posters", "logos", "backdrops", "episodes"] {
            count += rename_files(&std::path::Path::new(&cache_dir).join(subdir))?;
        }
        Ok::<_, AppError>(count)
    })
    .await
    .map_err(|e| AppError::Other(format!("rename task panicked: {e}")))?
    ?;
    tracing::info!(fs_renamed = renamed, "cache files renamed (added shape/background suffixes)");
    Ok(())
}

/// Recursively rename cached image files under `dir` to include the default
/// shape/background suffixes.
fn rename_files(dir: &std::path::Path) -> Result<u64, AppError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => {
            return Err(AppError::Other(format!(
                "failed to read {}: {e}",
                dir.display()
            )))
        }
    };

    let mut count = 0u64;
    for entry in entries {
        let entry = entry.map_err(|e| AppError::Other(e.to_string()))?;
        let path = entry.path();
        if path.is_dir() {
            count += rename_files(&path)?;
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(new_name) = migrate_name(name) {
                let new_path = path.with_file_name(new_name);
                std::fs::rename(&path, &new_path).map_err(|e| {
                    AppError::Other(format!(
                        "rename failed: {} → {}: {e}",
                        path.display(),
                        new_path.display()
                    ))
                })?;
                count += 1;
            }
        }
    }

    Ok(count)
}

/// Transform an old cache filename to include the default shape/background
/// suffixes, inserting `.shr.bgd` right after the badge size token.
///
/// Returns `None` if already migrated (contains a `.bg` background suffix) or no
/// badge size token is present (pre-badge-size orphans, already unreachable).
fn migrate_name(name: &str) -> Option<String> {
    if name.contains(".bg") {
        return None;
    }
    for token in SIZE_TOKENS {
        if name.contains(token) {
            let new = format!("{token}{DEFAULT_SUFFIX}");
            return Some(name.replacen(token, &new, 1));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_name_poster_default() {
        // Poster, medium badge, medium image size, default (rounded) look.
        assert_eq!(
            migrate_name("tt1234567@mil.pbc.sh.li.dh.bm.zm.jpg"),
            Some("tt1234567@mil.pbc.sh.li.dh.bm.shr.bgd.zm.jpg".to_string()),
        );
    }

    #[test]
    fn migrate_name_poster_with_split() {
        // Split posters keep their `.x1` token after the inserted shape/bg.
        assert_eq!(
            migrate_name("tt1234567@mil.pbc.sh.li.dh.bm.x1.zm.jpg"),
            Some("tt1234567@mil.pbc.sh.li.dh.bm.shr.bgd.x1.zm.jpg".to_string()),
        );
    }

    #[test]
    fn migrate_name_logo() {
        assert_eq!(
            migrate_name("tt1234567_l_t@mil.sv.lo.bm.zm.png"),
            Some("tt1234567_l_t@mil.sv.lo.bm.shr.bgd.zm.png".to_string()),
        );
    }

    #[test]
    fn migrate_name_backdrop() {
        assert_eq!(
            migrate_name("tt1234567_b_f@mil.ptr.sv.lt.dv.bm.zm.jpg"),
            Some("tt1234567_b_f@mil.ptr.sv.lt.dv.bm.shr.bgd.zm.jpg".to_string()),
        );
    }

    #[test]
    fn migrate_name_episode_with_blur() {
        // Blur token stays after the inserted shape/background suffixes.
        assert_eq!(
            migrate_name("tt1234567_e_t@m.ptr.sv.lo.dv.bl.blur.zm.jpg"),
            Some("tt1234567_e_t@m.ptr.sv.lo.dv.bl.shr.bgd.blur.zm.jpg".to_string()),
        );
    }

    #[test]
    fn migrate_name_extra_small_and_extra_large_sizes() {
        assert_eq!(
            migrate_name("tt1@m.pbc.sh.lt.dh.bxs.zs.jpg"),
            Some("tt1@m.pbc.sh.lt.dh.bxs.shr.bgd.zs.jpg".to_string()),
        );
        assert_eq!(
            migrate_name("tt1@m.pbc.sh.lt.dh.bxl.zvl.jpg"),
            Some("tt1@m.pbc.sh.lt.dh.bxl.shr.bgd.zvl.jpg".to_string()),
        );
    }

    #[test]
    fn migrate_name_bottom_left_large_does_not_match_position() {
        // Position bottom-left (`.pbl`) must not be mistaken for the large
        // badge size (`.bl`) — only the size token gets the suffix.
        assert_eq!(
            migrate_name("tt1234567_b_f@mil.pbl.sv.lt.dv.bl.zm.jpg"),
            Some("tt1234567_b_f@mil.pbl.sv.lt.dv.bl.shr.bgd.zm.jpg".to_string()),
        );
    }

    #[test]
    fn migrate_name_already_migrated() {
        assert_eq!(
            migrate_name("tt1234567@mil.pbc.sh.li.dh.bm.shr.bgd.zm.jpg"),
            None,
        );
    }

    #[test]
    fn migrate_name_no_size_token() {
        // Pre-badge-size orphan or unrelated file — left untouched.
        assert_eq!(migrate_name("tt1234567.jpg"), None);
        assert_eq!(migrate_name("abc123.jpg"), None);
    }
}
