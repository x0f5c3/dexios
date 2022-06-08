// this file handles getting parameters from clap's ArgMatches
// it returns information (e.g. CryptoParams) to functions that require it

use crate::global::states::{EraseMode, HashMode, HeaderFile, KeyFile, PasswordMode, SkipMode};
use crate::global::structs::CryptoParams;
use anyhow::{Context, Result};
use clap::ArgMatches;
use dexios_core::primitives::Algorithm;
use paris::warn;

use dexios_core::primitives::ALGORITHMS;

pub fn get_param(name: &str, sub_matches: &ArgMatches) -> Result<String> {
    let value = sub_matches
        .value_of(name)
        .with_context(|| format!("No {} provided", name))?
        .to_string();
    Ok(value)
}

pub fn parameter_handler(sub_matches: &ArgMatches) -> Result<CryptoParams> {
    let keyfile = if sub_matches.is_present("keyfile") {
        KeyFile::Some(
            sub_matches
                .value_of("keyfile")
                .context("No keyfile/invalid text provided")?
                .to_string(),
        )
    } else {
        KeyFile::None
    };

    let hash_mode = if sub_matches.is_present("hash") {
        //specify to emit hash after operation
        HashMode::CalculateHash
    } else {
        // default
        HashMode::NoHash
    };

    let skip = skipmode(sub_matches);

    let erase = if sub_matches.is_present("erase") {
        let result = sub_matches
            .value_of("erase")
            .context("No amount of passes specified")?
            .parse();

        let passes = if let Ok(value) = result {
            value
        } else {
            warn!("Unable to read number of passes provided - using the default.");
            2
        };
        EraseMode::EraseFile(passes)
    } else {
        EraseMode::IgnoreFile(0)
    };

    let password = if sub_matches.is_present("password") {
        //Overwrite, so the user provided password is used and ignore environment supplied one?!
        PasswordMode::ForceUserProvidedPassword
    } else {
        // default
        PasswordMode::NormalKeySourcePriority
    };

    Ok(CryptoParams {
        hash_mode,
        skip,
        password,
        erase,
        keyfile,
    })
}

pub fn encrypt_additional_params(sub_matches: &ArgMatches) -> Result<Algorithm> {
    let provided_aead: usize = if sub_matches.is_present("aead") {
        sub_matches
            .value_of("aead")
            .context("Error reading value of --aead")?
            .parse()
            .context(
                "Invalid AEAD selected! Use \"dexios list aead\" to see all possible values.",
            )? // add context here
    } else {
        1
    };

    if provided_aead < 1 || provided_aead > ALGORITHMS.len() {
        Err(anyhow::anyhow!(
            "Invalid AEAD selected! Use \"dexios list aead\" to see all possible values."
        ))
    } else {
        Ok(ALGORITHMS[provided_aead - 1]) // -1 to account for indexing starting at 0
    }
}

pub fn decrypt_additional_params(sub_matches: &ArgMatches) -> Result<HeaderFile> {
    let header = if sub_matches.is_present("header") {
        HeaderFile::Some(
            sub_matches
                .value_of("header")
                .context("No header/invalid text provided")?
                .to_string(),
        )
    } else {
        HeaderFile::None
    };

    Ok(header)
}

pub fn erase_params(sub_matches: &ArgMatches) -> Result<i32> {
    let passes = if sub_matches.is_present("passes") {
        let result = sub_matches
            .value_of("passes")
            .context("No amount of passes specified")?
            .parse::<i32>();
        if let Ok(value) = result {
            value
        } else {
            warn!("Unable to read number of passes provided - using the default.");
            2
        }
    } else {
        warn!("Number of passes not provided - using the default.");
        2
    };

    Ok(passes)
}

pub fn skipmode(sub_matches: &ArgMatches) -> SkipMode {
    if sub_matches.is_present("skip") {
        SkipMode::HidePrompts
    } else {
        SkipMode::ShowPrompts
    }
}
