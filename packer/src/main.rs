/*
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

mod error;
mod types;

use clap::value_t_or_exit;
use std::collections::BTreeMap;
use std::fs::File;
use std::path;
use types::{PackType, Platform};

const DEFAULT_PACKLIST: &str = include_str!("packlist.yaml");

type PackListPlatform = BTreeMap<PackType, Vec<path::PathBuf>>;

#[derive(Debug, serde::Deserialize)]
struct PackList(pub BTreeMap<Platform, PackListPlatform>);

fn pack(
    platform: &Platform,
    dist_dir: &std::path::PathBuf,
    pack_list: &PackList,
    output_directory: &std::path::PathBuf,
) -> Result<(), error::Error> {
    let mut frameworks_path = output_directory.clone();
    frameworks_path.push("frameworks.tar");
    let mut frameworks_tar = tar::Builder::new(File::create(frameworks_path)?);
    // MacOS uses symlinks for bundling multiple framework versions and pointing
    // to the "Current" one.
    frameworks_tar.follow_symlinks(false);
    pack_platform(
        platform,
        dist_dir,
        pack_list,
        &PackType::Frameworks,
        &mut frameworks_tar,
    )?;
    frameworks_tar.finish()?;

    // TODO: Instead of hard-coding the two types, just iterate over the packlist.
    let mut core_path = output_directory.clone();
    core_path.push("core.tar");
    let mut core_tar = tar::Builder::new(File::create(core_path)?);
    pack_platform(
        platform,
        dist_dir,
        pack_list,
        &PackType::Core,
        &mut core_tar,
    )?;

    eprintln!("Written.");
    Ok(())
}

fn pack_platform(
    platform: &Platform,
    dist_dir: &std::path::PathBuf,
    pack_list: &PackList,
    pack_type: &PackType,
    tar_builder: &mut tar::Builder<File>,
) -> Result<(), error::Error> {
    let pack_files = pack_list
        .0
        .get(platform)
        .and_then(|f| f.get(pack_type))
        .expect(&format!(
            "Missing packlist definition for platform {:?} and pack type {:?}.",
            platform, pack_type
        ));
    let base_dir = match platform {
        Platform::Mac => path::Path::new(dist_dir).join("mac"),
        // TODO: Verify this.
        Platform::Linux => path::Path::new(dist_dir).join("linux-unpacked"),
        Platform::Windows => path::Path::new(dist_dir).join("win-unpacked"),
    };

    for f in pack_files {
        let full_path = path::Path::new(&base_dir).join(f);
        if !full_path.exists() {
            return Err(error::Error::MissingPackFile(
                platform.clone(),
                pack_type.clone(),
                full_path,
            ));
        }
        if full_path.is_file() {
            tar_builder.append_path_with_name(full_path, f)?;
        } else if full_path.is_dir() {
            tar_builder.append_dir_all(f, full_path)?;
        }
    }

    Ok(())
}

fn main() -> Result<(), error::Error> {
    // Ensure to define all env vars used here in the BUCK env, too.
    let args = clap::App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about("Split the Flipper distribution into smaller, cacheable artifacts")
        .arg(
            clap::Arg::from_usage("-o, --output [DIRECTORY] 'Directory to write output files to.'")
                .default_value("."),
        )
        .arg(
            clap::Arg::from_usage("-d, --dist [DIRECTORY] 'Flipper dist directory to read from.'")
                .default_value("~/fbsource/xplat/sonar/dist"),
        )
        .arg(clap::Arg::from_usage(
            "-p, --packlist=packlist.yaml 'Custom list of files to pack.'",
        ))
        .arg(
            clap::Arg::from_usage("[PLATFORM] 'Platform to build for'")
                .case_insensitive(true)
                .required(true)
                .possible_values(&Platform::variants()),
        )
        .get_matches();

    let platform = value_t_or_exit!(args.value_of("PLATFORM"), Platform);
    let dist_dir = path::PathBuf::from(
        shellexpand::tilde(args.value_of("dist").expect("argument has default")).to_string(),
    );
    let pack_list_str = args
        .value_of("packlist")
        .map(|f| std::fs::read_to_string(f).expect(&format!("Failed to open packfile {}.", f)))
        .unwrap_or(DEFAULT_PACKLIST.to_string());
    let pack_list: PackList =
        serde_yaml::from_str(&pack_list_str).expect("Failed to deserialize YAML packlist.");
    pack(
        &platform,
        &dist_dir,
        &pack_list,
        &path::PathBuf::from(args.value_of("output").expect("argument has default")),
    )
    .unwrap();

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_included_packlist_parses() {
        let res: PackList =
            serde_yaml::from_str(DEFAULT_PACKLIST).expect("Default packlist doesn't deserialize");
        assert_eq!(res.0.len(), 3);
    }
}
