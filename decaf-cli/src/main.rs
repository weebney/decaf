use std::time::Instant;
use std::{env, fs::File, path::Path, process::exit};

use decaf::*;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 || args.len() > 3 {
        usage();
        exit(1)
    }

    let input = args[1].as_str();
    let output = if args.len() == 3 {
        args[2].to_string()
    } else {
        if let Some(stripped) = input.strip_suffix(".df") {
            stripped.to_string()
        } else {
            let input_filename = Path::new(input).file_name().unwrap().to_str().unwrap();
            format!("{}.df", input_filename)
        }
    };

    if !input.ends_with(".df") {
        let timer_overall = Instant::now();
        // todo: spinners
        println!("decaf: indexing files in {}", input);
        let listings = decaf::create_listings_from_directory(Path::new(input)).unwrap();

        println!(
            "decaf: indexed {} files in {:.2} sec",
            listings.len(),
            timer_overall.elapsed().as_secs_f32()
        );

        println!("decaf: creating archive for {}", input);
        let mut outfile = File::create(output.clone()).unwrap();
        let bytes = listings.create_archive(&mut outfile).unwrap();

        println!(
            "decaf: archived {} as {} (wrote {:.2} mb) in {:.2} sec",
            input,
            output,
            bytes as f32 / 1024.0 / 1024.0,
            timer_overall.elapsed().as_secs_f32()
        );
    } else {
        let timer_overall = Instant::now();
        let mut infile = File::open(input).unwrap();
        println!("decaf: extracting files from archive {}", input);
        let listings = unarchive_to_listings(&mut infile).unwrap();
        println!(
            "decaf: extracted {} files in {:.2} sec",
            listings.len(),
            timer_overall.elapsed().as_secs_f32()
        );
        listings.create_files(output.clone()).unwrap();
        println!(
            "decaf: unarchived {} to {} in {:.2} sec",
            input,
            output,
            timer_overall.elapsed().as_secs_f32()
        );
    }
}

fn usage() {
    print!("decaf {}: {}", env! {"CARGO_PKG_VERSION"}, USAGE,);
}

static USAGE: &str = "manipulate DeCAF archives

Usage: df (ARCHIVE | DIRECTORY) [OUTPUT]

Arguments:
    <ARCHIVE | DIRECTORY>  Path to the input archive (.df) or directory
    [OUTPUT]               Optional path for output file or directory

Examples:
    Archiving:
        Create an archive from a directory:
            $ decaf my-folder/
        This will create an archive `my-folder.df` in the current directory.

        Creating an archive to a specific output file:
            $ decaf my-folder/ output.df
        This will create an archive from `my-folder` as `output.df`.

    Unarchiving:
        Unarchiving to a directory:
            $ decaf photos.df
        This will create a directory `photos/` in the current directory.

        Unarchiving to a specific directory:
            $ decaf photos.df pictures/
        This will create a directory `pictures/` from the archive `photos.df` in the current directory.

Copyright (c) The DeCAF Project Developers, 2024. Licensed MIT OR Apache-2.0 OR BSD-2-Clause.
";
