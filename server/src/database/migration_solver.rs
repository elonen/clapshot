use std::collections::{HashMap, HashSet};
use lib_clapshot_grpc::proto::org::Migration;

pub struct MigrationGraphModule {
	pub name: String,				    // Unique name (id) of this module
	pub cur_version: Option<String>,    // Current migration version this modules is at
	pub migrations: Vec<Migration>,	    // Available (alternative) migrations for this module
}

/// For a given set of modules and their migrations, find a valid path of migrations that
/// upgrades all modules to their latest version.
///
/// Returns `None` if no solution was found.
pub fn solve_migration_graph(modules: Vec<&MigrationGraphModule>) -> anyhow::Result<Option<Vec<Migration>>> {
    assert!(!modules.is_empty());

    let mut cur_module_versions = HashMap::new();   // name -> version
    let mut target_module_versions = HashMap::new();

    // Initialize the current and max versions for each module
    for module in &modules {
        if let Some(cur_version) = &module.cur_version {
            cur_module_versions.insert(module.name.as_str(), cur_version.as_str());
        }
        if let Some(max_version) = module.migrations.iter().max_by_key(|m| &m.version).map(|m| m.version.as_str()) {
            target_module_versions.insert(module.name.as_str(), max_version);
        }
    }

    // List all migrations that advance the current version of some module as a tuple (module_name, migration)
    let mut all_migrations: Vec<(&str, &Migration)> = modules.iter()
        .flat_map(|module| module.migrations.iter()
            .filter(|mig| module.cur_version.is_none() || mig.version.as_str() > module.cur_version.as_ref().unwrap().as_str())
            .map(|mig| (module.name.as_str(), mig))
        ).collect();

    // Check that uuids are unique
    let mut uuids = HashSet::new();
    for (_, mig) in &all_migrations {
        if !uuids.insert(mig.uuid.as_str()) {
            anyhow::bail!("Duplicate migration UUID: {}", mig.uuid);
        }
    }

    all_migrations.sort_by(|a, b| a.1.version.cmp(&b.1.version));  // Oldest versions first

    let mut solution = None;
    depth_first_search(&all_migrations, &target_module_versions, cur_module_versions.clone(), HashSet::new(), vec![], &mut solution);

    Ok(solution.map(|path| { path.into_iter().cloned().collect() }))
}


/// Recursive depth-first search for shortest path of migrations
/// that upgrades all modules to their target version.
fn depth_first_search<'a>(
    all_migrations: &'a Vec<(&'a str, &'a Migration)>,      // (module_name, migration)
    max_module_versions: &'a HashMap<&'a str, &'a str>,     // module_name -> target version
    cur_module_versions: HashMap<&'a str, &'a str>,         // module_name -> current version
    visited: HashSet<&'a str>,                              // Set of visited migration UUIDs
    cur_path: Vec<&'a Migration>,                           // Current path of migrations (in reverse order)
    best_path: &mut Option<Vec<&'a Migration>>,             // Best path found so far (in reverse order)
) {
    // Solution found? (all modules are at their target version)
    if max_module_versions.iter().all(|(mod_name, max_ver)| {
        cur_module_versions.get(mod_name).map_or(false, |cur_ver| cur_ver == max_ver) })
    {
        if best_path.is_none() || cur_path.len() < best_path.as_ref().unwrap().len() {
            *best_path = Some(cur_path.iter().cloned().collect());
        }
        return;
    }

    // No, continue searching
    for (mod_name, mig) in all_migrations {
        if visited.contains(mig.uuid.as_str()) {
            continue;
        }

        let mut can_apply = true;
        for dep in &mig.dependencies {
            if let Some(cur_version) = cur_module_versions.get(dep.name.as_str()) {
                // Module has a current version => check for min/max version requirements
                if let Some(min_ver) = &dep.min_ver {
                    can_apply &= *cur_version >= min_ver.as_str();
                }
                if let Some(max_ver) = &dep.max_ver {
                    can_apply &= *cur_version <= max_ver.as_str();
                }
            } else {
                // If module doesn't have a current version, it means it hasn't applied any migrations yet.
                // We can apply the migration if it doesn't have a min version requirement.
                can_apply &= dep.min_ver.is_none();
            }
        }

        if can_apply {
            let mut new_path = cur_path.clone();
            new_path.push(mig);

            let mut new_cur_module_versions = cur_module_versions.clone();
            new_cur_module_versions.insert(mod_name, mig.version.as_str());

            let mut new_visited = visited.clone();
            new_visited.insert(mig.uuid.as_str());

            depth_first_search(all_migrations, max_module_versions, new_cur_module_versions, new_visited, new_path, best_path);
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use lib_clapshot_grpc::proto::org::migration::Dependency;

    macro_rules! migmod {
        ($name:expr, $cur_ver:expr, $migs:expr) => {
            MigrationGraphModule { name: $name.to_string(), cur_version: $cur_ver.map(|s: &str| s.to_string()), migrations: $migs }
        };
    }
    macro_rules! mig {
        ($uuid:expr, $ver:expr, $deps:expr) => {
            Migration { uuid: $uuid.to_string(), version: $ver.to_string(), dependencies: $deps, description: "dummy-desc".to_string() }
        };
    }
    macro_rules! dep {
        ($name:expr, $min:expr, $max:expr) => {
            Dependency { name: $name.to_string(), min_ver: $min.map(|s: &str| s.to_string()), max_ver: $max.map(|s: &str| s.to_string()) }
        };
    }

    fn compare_results(result: &Option<Vec<Migration>>, expected: Option<Vec<&str>>) {
        match expected {
            None => {
                assert!(result.is_none(), "Expected None, got: {:?}",
                    result.clone().unwrap().iter().map(|m| m.uuid.as_str()).collect::<Vec<_>>());
            }
            Some(expected) => {
                let result = result.as_ref().unwrap();
                let eq = result.iter().zip(expected.iter()).all(|(a, b)| &a.uuid == b);
                assert!(eq, "Expected:\n{:?}\nGot:\n{:?}",
                    expected.iter().map(|m| m).collect::<Vec<_>>(),
                    result.iter().map(|m| m.uuid.as_str()).collect::<Vec<_>>()
                );
            }
        }
    }

    fn solve_and_compare(modules: Vec<&MigrationGraphModule>, expected: Option<Vec<&str>>) {
        let result = solve_migration_graph(modules).expect("solve_migration_graph failed");
        compare_results(&result, expected);
    }

    // ------------------------------------------------------------------------

    #[test]
    fn test_msolv_trivial_from_empty() {
        let mod_server = migmod!("server", None, vec![
            mig!("uuid1", "1", vec![]),
            mig!("uuid2", "2", vec![dep!("server", Some("1"), Some("1"))]),
            mig!("uuid3", "3", vec![dep!("server", Some("2"), Some("2"))]),
        ]);
        let correct = vec![ "uuid1", "uuid2", "uuid3" ];
        solve_and_compare(vec![&mod_server], Some(correct));
    }

    #[test]
    fn test_msolv_shortcut() {
        let mod_server = migmod!("server", Some("1"), vec![
            mig!("uuid1", "1", vec![]),
            mig!("uuid2", "2", vec![dep!("server", Some("1"), Some("1"))]),
            mig!("uuid3", "3", vec![dep!("server", Some("2"), Some("2"))]),
            mig!("uuid4", "4", vec![dep!("server", Some("1"), Some("3"))]),
        ]);
        let correct = vec![ "uuid4" ];
        solve_and_compare(vec![&mod_server], Some(correct));
    }

    #[test]
    fn test_msolv_two_modules_indep() {
        let mod_server = migmod!("server", None, vec![
            mig!("S1", "1", vec![]),
            mig!("S2", "2", vec![dep!("server", Some("1"), Some("1"))]),
            mig!("S3", "3", vec![dep!("server", Some("2"), Some("2"))]),
        ]);
        let mod_org = migmod!("org", Some("0"), vec![
            mig!("G1", "1", vec![dep!("org", None, Some("0"))]),
            mig!("G2", "2", vec![dep!("org", Some("1"), Some("1"))]),
            mig!("G3", "3", vec![dep!("org", Some("2"), Some("2"))]),
        ]);
        let correct = vec![ "S1", "G1", "S2", "G2", "S3", "G3" ];
        solve_and_compare(vec![&mod_server, &mod_org], Some(correct));
    }

    #[test]
    fn test_msolv_two_modules_dep() {
        let mod_server = migmod!("server", None, vec![
            mig!("S1", "1", vec![]),
            mig!("S2", "2", vec![dep!("server", Some("1"), Some("1"))]),
            mig!("S3", "3", vec![dep!("server", Some("2"), Some("2"))]),
        ]);
        let mod_org = migmod!("org", Some("0"), vec![
            mig!("G1", "1", vec![
                dep!("org", Some("0"), Some("0")),
                dep!("server", None, Some("1"))]),
            mig!("G2", "2", vec![
                dep!("org", Some("1"), Some("1")),
                dep!("server", Some("2"), Some("2"))]),
            mig!("G3", "3", vec![
                dep!("org", Some("2"), Some("2")),
                dep!("server", Some("2"), Some("2"))]),
        ]);
        let correct = vec!["S1", "G1", "S2", "G2", "G3", "S3"];
        solve_and_compare(vec![&mod_server, &mod_org], Some(correct));
    }

    #[test]
    fn test_msolv_one_module_nonsolvable() {
        let mod_server = migmod!("server", None, vec![
            mig!("S1", "1", vec![]),
            mig!("S2", "2", vec![dep!("server", Some("1"), Some("1"))]),
            // missing migration to version 3
            mig!("S4", "4", vec![dep!("server", Some("3"), Some("3"))]),
        ]);
        solve_and_compare(vec![&mod_server], None);
    }


    #[test]
    fn test_msolv_two_modules_nonsolvable() {
        let mod_server = migmod!("server", None, vec![
            mig!("S1", "1", vec![]),
            mig!("S2", "2", vec![dep!("server", Some("1"), Some("1"))]),
            mig!("S3", "3", vec![dep!("server", Some("2"), Some("2"))]),
        ]);
        let mod_org = migmod!("org", Some("0"), vec![
            mig!("G1", "1", vec![
                dep!("org", Some("0"), Some("0")),
                dep!("server", Some("1"), Some("1"))]),
            mig!("G2", "2", vec![
                dep!("org", Some("2"), Some("2"))]),
            mig!("G3", "3", vec![
                dep!("org", Some("2"), Some("2")),
                dep!("server", Some("1"), Some("1"))]),
        ]);
        solve_and_compare(vec![&mod_server, &mod_org], None);
    }

    #[test]
    fn test_actual_failing_case() {
        /*
        Migration: '2023-04-18-190209_change_video_primkey' of module 'clapshot.server' depends on: '[Dependency { name: "clapshot.server", min_ver: None, max_ver: Some("") }]')
        Migration: '2023-04-18-190300_add_cascade_rules' of module 'clapshot.server' depends on: '[Dependency { name: "clapshot.server", min_ver: Some("2023-04-18-190209_change_video_primkey"), max_ver: Some("2023-04-18-190209_change_video_primkey") }]')
        Migration: '2024-05-13-093800_add_users_table' of module 'clapshot.server' depends on: '[Dependency { name: "clapshot.server", min_ver: Some("2023-04-18-190300_add_cascade_rules"), max_ver: Some("2023-04-18-190300_add_cascade_rules") }]')
        Migration: '2024-05-22-163000_add_media_type' of module 'clapshot.server' depends on: '[Dependency { name: "clapshot.server", min_ver: Some("2024-05-13-093800_add_users_table"), max_ver: Some("2024-05-13-093800_add_users_table") }]')
        Migration: '2024-05-30-202000_add_missing_users' of module 'clapshot.server' depends on: '[Dependency { name: "clapshot.server", min_ver: Some("2024-05-22-163000_add_media_type"), max_ver: Some("2024-05-22-163000_add_media_type") }]')
        */
        let mod_server = migmod!("clapshot.server", Some("1"), vec![
            mig!("2023-04-18-190209_change_video_primkey", "2023-04-18-190209_change_video_primkey", vec![dep!("clapshot.server", None, Some(""))]),
            mig!("2023-04-18-190300_add_cascade_rules", "2023-04-18-190300_add_cascade_rules", vec![dep!("clapshot.server", Some("2023-04-18-190209_change_video_primkey"), Some("2023-04-18-190209_change_video_primkey"))]),
            mig!("2024-05-13-093800_add_users_table", "2024-05-13-093800_add_users_table", vec![dep!("clapshot.server", Some("2023-04-18-190300_add_cascade_rules"), Some("2023-04-18-190300_add_cascade_rules"))]),
            mig!("2024-05-22-163000_add_media_type", "2024-05-22-163000_add_media_type", vec![dep!("clapshot.server", Some("2024-05-13-093800_add_users_table"), Some("2024-05-13-093800_add_users_table"))]),
            mig!("2024-05-30-202000_add_missing_users", "2024-05-30-202000_add_missing_users", vec![dep!("clapshot.server", Some("2024-05-22-163000_add_media_type"), Some("2024-05-22-163000_add_media_type"))]),
        ]);
        solve_and_compare(vec![&mod_server], None);
    }

}
