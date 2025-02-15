use std::{error::Error, fmt::Display, io::BufWriter, path::PathBuf};

use clap::{ArgGroup, Parser, Subcommand};
#[cfg(not(test))]
use mimalloc::MiMalloc;

use std::io::Write;

mod contributors;
mod prune;
mod remove;

#[cfg(not(test))]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

/// CLI tool for reading and rewriting history information of a git repository
#[derive(clap::Parser)]
struct Cli {
    /// Path to the mirrored/bare repository (do not use on a repository with a working copy)
    repository: Option<String>,

    #[command(subcommand)]
    command: Commands,

    /// Do not change the repository.
    #[arg(short, long)]
    dry_run: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Contributor related actions like list and rewrite
    #[command(subcommand)]
    Contributor(ContributorArgs),

    /// Remove files and whole directories from the repository
    #[command(group(ArgGroup::new("input")
                        .required(true)
                        .multiple(true)))]
    Remove {
        /// File to remove. The char '*' can be used as a wildcard at the beginning or end. Path can be absolute or relative, depending on if a '/' is present. Argument can be specified multiple times
        #[arg(short, long, group = "input")]
        file: Option<Vec<String>>,

        /// Directory to remove. The char '*' can be used as a wildcard at the begining or end. Path can be absolute or relative, depending on if it starts with a '/'. Argument can be specified multiple times
        #[arg(short, long, group = "input")]
        directory: Option<Vec<String>>,

        /// Regex to remove files. Matches on the whole path including the filename, which makes it a little more expensive than the file or directory options. Argument can be specified multiple times
        #[arg(short, long, group = "input")]
        regex: Option<Vec<String>>,
    },

    /// Remove empty commits that are no merge commits
    PruneEmpty,

    #[command(hide = true)]
    MarkdownHelp,
}

#[derive(Subcommand)]
enum ContributorArgs {
    /// Lists all authors and committers
    List,
    /// Allows to rewrite contributors. Expects stdin input lines with the format: Old User <old@user.mail> = New User <new@user.mail>
    Rewrite,
}

fn main() {
    let cli = Cli::parse();
    let repository_path = PathBuf::from(cli.repository.unwrap_or(String::from(".")));

    match cli.command {
        Commands::Contributor(args) => match args {
            ContributorArgs::List => {
                print_locked(
                    contributors::get_contributors(repository_path)
                        .unwrap()
                        .iter(),
                )
                .unwrap();
            }
            ContributorArgs::Rewrite => {
                contributors::rewrite(repository_path, cli.dry_run).unwrap();
            }
        },
        Commands::Remove {
            file,
            directory,
            regex,
        } => {
            remove::remove(
                repository_path,
                file.unwrap_or_default(),
                directory.unwrap_or_default(),
                regex.unwrap_or_default(),
                cli.dry_run,
            );
        }

        Commands::PruneEmpty => {
            prune::remove_empty_commits(repository_path, cli.dry_run).unwrap();
        },

        Commands::MarkdownHelp => {
            clap_markdown::print_help_markdown::<Cli>();
        }
    };
}

fn print_locked<T: Display>(items: impl Iterator<Item = T>) -> Result<(), Box<dyn Error>> {
    let lock = std::io::stdout().lock();
    let mut handle = BufWriter::new(lock);

    for item in items {
        writeln!(handle, "{item}")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use bstr::ByteSlice;
    use gitrwlib::objs::{CommitBase, CommitEditable, CommitHash};

    const BYTES: &[u8] = b"tree 31aa860596f003d69b896943677e9fe5ff208233\nparent 5eec99927bb6058c8180e5dac871c89c7d01b0ab\nauthor Tim Heinrich <2929650+TimHeinrich@users.noreply.github.com> 1688207675 +0200\ncommitter Tim Heinrich <2929650+TimHeinrich@users.noreply.github.com> 1688209149 +0200\n\nChanging of commit data\n";

    #[test]
    fn miri_commit() {
        let object_hash: CommitHash = b"53dd2e51161a4eebd8baacd17383c9af35a8283e"
            .as_bstr()
            .try_into()
            .unwrap();

        let mut commit =
            CommitEditable::create(CommitBase::create(object_hash, BYTES.into(), false));

        let author = commit.author().to_owned();
        commit.set_author(b"Test user".to_vec());

        let (sender, receiver) = channel();

        let thread = std::thread::spawn(move || {
            sender.send(commit).unwrap();
        });

        for mut commit in receiver {
            assert_eq!("Test user", commit.author());
            commit.set_author(author.clone().bytes().collect());
            let b = commit.to_bytes();

            assert_eq!(BYTES.to_vec().into_boxed_slice(), b.get_bytes().into());
        }

        thread.join().unwrap();
    }
}
