use chrono::Local;
use clap::{ArgEnum, Parser};
use git2::Repository;
use uuid::Uuid;

use std::ffi::OsString;
use std::fs::create_dir;
use std::path::Path;
use std::process::Command;

const DIRECTORIES: [&str; 7] = ["src", "raw", "results", "paper", "tmp", "cache", "choices"];

fn write(path: &Path, string: &str) {
    std::fs::write(path, string.as_bytes())
        .unwrap_or_else(|_| panic!("Error when creating file {:?}", path))
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => "".to_owned(),
        Some(char) => {
            let mut result: String = char.to_uppercase().collect();
            result.extend(chars);
            result
        }
    }
}

fn make_readme(path: &Path, project_name: &str, author: &Option<String>) {
    let date = Local::today().format("%Y-%m-%d").to_string();

    // Add top of Readme
    let mut content = format!(
        include_str!("../templates/readme"),
        project_name = project_name,
        author = match author {
            None => "".to_owned(),
            Some(name) => format!("Author: {}\n", name),
        },
        date = date
    );

    // Add directory content, taken from main README.md...
    let readme = include_str!("../README.md");
    let mut lines = readme.lines();
    // ... From first "# Directory structure" ...
    for line in lines.by_ref() {
        if line.trim() == "## Directory structure" {
            content.push_str(line);
            content.push('\n');
            break;
        }
    }
    // ... to next header.
    for line in lines {
        if line.starts_with("## ") {
            break;
        }
        content.push_str(line);
        content.push('\n');
    }
    write(path, &content)
}

fn convert_name_to_module(project_name: &str) -> String {
    // Splits by dash or underscore, then capitalize each chunk before joining.
    project_name
        .split(|c| c == '_' || c == '-')
        .map(capitalize)
        .collect()
}

fn make_julia_project(path: &Path, module_name: &str, author_email: &Option<(String, String)>) {
    let author_string = match author_email {
        None => "Unknown author".to_owned(),
        Some((name, mail)) => format!("{} <{}>", &name, &mail),
    };
    let uuid = Uuid::new_v4().hyphenated().to_string();
    let content = format!(
        include_str!("../templates/project"),
        module_name = module_name,
        uuid_str = uuid,
        author = author_string
    );
    write(path, &content)
}

fn conda_create(project_name: &str) {
    match Command::new("conda")
        .args(["create", "-n", project_name, "-y"])
        .output()
    {
        Ok(_) => println!("Created Conda environment \"{}\"", &project_name),
        Err(_) => eprintln!(
            "Warning: Could not create Conda environment \"{}\"",
            &project_name
        ),
    }
}

fn make_conda_yml(path: &Path, project_name: &str) {
    let prefix = match std::env::var("CONDA_PREFIX") {
        Err(_) => {
            eprintln!("Warning: Could not get env variable $CONDA_PREFIX. Not writing \"environment.yml\" file.");
            return;
        }
        Ok(x) => x,
    };
    let prefix_path = Path::new(&prefix).join("envs").join(project_name);
    write(
        &path.join("environment.yml"),
        &format!(
            include_str!("../templates/environment"),
            name = project_name,
            prefix_path = prefix_path.to_str().unwrap()
        ),
    );
}

fn make_dirs(path: &Path) {
    create_dir(path)
        .unwrap_or_else(|_| panic!("Error when creating main project directory: {:?}", path));
    for subdir in DIRECTORIES {
        create_dir(path.join(subdir))
            .unwrap_or_else(|_| panic!("Error when creating sub-directory: {:?}", path));
    }
}

fn get_author_email() -> Option<(String, String)> {
    let mut name = None;
    let mut email = None;
    let config = git2::Config::open_default().ok()?;
    for maybe_entry in &config.entries(Some("user")).ok()? {
        let entry = maybe_entry.ok()?;
        let entryname = entry.name()?;
        let value = entry.value()?;
        if entryname == "user.name" {
            name = Some(value.to_owned())
        } else if entryname == "user.email" {
            email = Some(value.to_owned())
        }
    }
    Some((name?, email?))
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum)]
enum Language {
    Python,
    Julia,
}

#[derive(Parser)]
#[clap(version, author, about)]
struct Options {
    /// Path to project dir to create
    dirname: OsString, // if None, try to init current dir

    /// Main programming language
    #[clap(arg_enum, value_parser, short, long)]
    language: Option<Language>,

    /// Project name (default: same as <DIRNAME>)
    #[clap(short, long)]
    name: Option<String>,
}

fn main() {
    let args = Options::parse();
    let path = Path::new(&args.dirname);
    let project_name = if let Some(name) = args.name {
        name
    } else {
        args.dirname
            .to_str()
            .unwrap_or_else(|| {
                eprint!(
                    "Error: Project name {:?} is not a normal UTF-8 string",
                    args.dirname
                );
                std::process::exit(1)
            })
            .to_owned()
    };
    if project_name.is_empty() {
        eprint!("Error: Project name cannot be empty");
        std::process::exit(1)
    }
    let capitalized_project = capitalize(&project_name);
    make_dirs(path);
    Repository::init(&path).expect("Error when initializing git repo:");
    let author_email = get_author_email();
    if author_email.is_none() {
        eprintln!(
            "Warning: Could not extract author name and email from global git config.\n\
            Set name and mail with:\n\
            git config --global user.name \"FIRST_NAME LAST_NAME\"\n\
            git config --global user.mail \"EXAMPLE@EMAIL.COM\"\n"
        )
    }
    let author = author_email.as_ref().map(|x| x.0.clone());

    // .gitignore
    let python_gitignore = match args.language {
        Some(Language::Python) => "__pycache__",
        _ => "",
    };
    write(
        &path.join(".gitignore"),
        &format!(
            include_str!("../templates/gitignore"),
            python_gitignore = python_gitignore
        ),
    );

    // Readme
    make_readme(&path.join("README.md"), &capitalized_project, &author);

    // Extra Python/Julia specifics
    if let Some(language) = args.language {
        match language {
            Language::Julia => {
                let module_name = convert_name_to_module(&project_name);
                write(
                    &path.join("src").join(module_name.clone() + ".jl"),
                    include_str!("../templates/main"),
                );
                make_julia_project(&path.join("Project.toml"), &module_name, &author_email);
            }
            Language::Python => {
                write(
                    &path.join("src").join("main.py"),
                    include_str!("../templates/main"),
                );
                conda_create(&project_name);
                make_conda_yml(path, &project_name);
            }
        }
    }
}
