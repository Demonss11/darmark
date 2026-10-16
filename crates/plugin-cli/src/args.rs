//! Разбор аргументов по командам. Диспетчер верхнего уровня — в `main.rs`.

use std::path::PathBuf;

use crate::Action;

pub fn parse_test_args(args: &[String]) -> Result<Action, String> {
    let mut path: Option<PathBuf> = None;
    let mut json = false;
    let mut verbose = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--verbose" => verbose = true,
            other if other.starts_with('-') => {
                return Err(format!("неизвестный аргумент: {other}"))
            }
            other => {
                if path.is_some() {
                    return Err("укажите ровно один путь".to_string());
                }
                path = Some(PathBuf::from(other));
            }
        }
    }
    let path = path.ok_or_else(|| "test требует путь <path>".to_string())?;
    Ok(Action::Test {
        path,
        json,
        verbose,
    })
}

pub fn parse_init_args(args: &[String]) -> Result<Action, String> {
    let mut dir: Option<PathBuf> = None;
    let mut id: Option<String> = None;
    let mut name: Option<String> = None;
    let mut force = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--force" => force = true,
            "--id" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--id требует значение".to_string())?;
                id = Some(value.clone());
            }
            "--name" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--name требует значение".to_string())?;
                name = Some(value.clone());
            }
            other if other.starts_with('-') => {
                return Err(format!("неизвестный аргумент: {other}"))
            }
            other => {
                if dir.is_some() {
                    return Err("укажите ровно один каталог".to_string());
                }
                dir = Some(PathBuf::from(other));
            }
        }
    }
    let dir = dir.ok_or_else(|| "init требует каталог <dir>".to_string())?;
    Ok(Action::Init {
        dir,
        id,
        name,
        force,
    })
}

pub fn parse_list_args(args: &[String]) -> Result<Action, String> {
    let mut repo: Option<PathBuf> = None;
    let mut json = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--repo" => repo = Some(PathBuf::from(take_value(&mut iter, "--repo")?)),
            other if other.starts_with('-') => {
                return Err(format!("неизвестный аргумент: {other}"))
            }
            other => return Err(format!("лишний аргумент: {other}")),
        }
    }
    Ok(Action::List { repo, json })
}

pub fn parse_install_args(args: &[String]) -> Result<Action, String> {
    let (id, repo, dest, force) = parse_transfer_args(args, "install")?;
    Ok(Action::Install {
        id,
        repo,
        dest,
        force,
    })
}

pub fn parse_link_args(args: &[String]) -> Result<Action, String> {
    let (id, repo, dest, force) = parse_transfer_args(args, "link")?;
    Ok(Action::Link {
        id,
        repo,
        dest,
        force,
    })
}

pub fn parse_remove_args(args: &[String]) -> Result<Action, String> {
    let mut id: Option<String> = None;
    let mut dest: Option<PathBuf> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--dest" => dest = Some(PathBuf::from(take_value(&mut iter, "--dest")?)),
            other if other.starts_with('-') => {
                return Err(format!("неизвестный аргумент: {other}"))
            }
            other => {
                if id.is_some() {
                    return Err("remove: укажите ровно один id".to_string());
                }
                id = Some(other.to_string());
            }
        }
    }
    let id = id.ok_or_else(|| "remove требует id <id>".to_string())?;
    Ok(Action::Remove { id, dest })
}

pub fn parse_update_args(args: &[String]) -> Result<Action, String> {
    let mut id: Option<String> = None;
    let mut repo: Option<PathBuf> = None;
    let mut dest: Option<PathBuf> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--repo" => repo = Some(PathBuf::from(take_value(&mut iter, "--repo")?)),
            "--dest" => dest = Some(PathBuf::from(take_value(&mut iter, "--dest")?)),
            other if other.starts_with('-') => {
                return Err(format!("неизвестный аргумент: {other}"))
            }
            other => {
                if id.is_some() {
                    return Err("update: укажите не более одного id".to_string());
                }
                id = Some(other.to_string());
            }
        }
    }
    Ok(Action::Update { id, repo, dest })
}


fn parse_transfer_args(
    args: &[String],
    command: &str,
) -> Result<(String, Option<PathBuf>, Option<PathBuf>, bool), String> {
    let mut id: Option<String> = None;
    let mut repo: Option<PathBuf> = None;
    let mut dest: Option<PathBuf> = None;
    let mut force = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--force" => force = true,
            "--repo" => repo = Some(PathBuf::from(take_value(&mut iter, "--repo")?)),
            "--dest" => dest = Some(PathBuf::from(take_value(&mut iter, "--dest")?)),
            other if other.starts_with('-') => {
                return Err(format!("неизвестный аргумент: {other}"))
            }
            other => {
                if id.is_some() {
                    return Err(format!("{command}: укажите ровно один id"));
                }
                id = Some(other.to_string());
            }
        }
    }
    let id = id.ok_or_else(|| format!("{command} требует id <id>"))?;
    Ok((id, repo, dest, force))
}

fn take_value(iter: &mut std::slice::Iter<'_, String>, flag: &str) -> Result<String, String> {
    iter.next()
        .cloned()
        .ok_or_else(|| format!("{flag} требует значение"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn err(action: Result<Action, String>) -> String {
        action.expect_err("ожидалась ошибка разбора")
    }

    #[test]
    fn test_requires_path() {
        assert_eq!(
            err(parse_test_args(&args(&["--json"]))),
            "test требует путь <path>"
        );
    }

    #[test]
    fn rejects_second_path() {
        assert_eq!(
            err(parse_test_args(&args(&["a", "b"]))),
            "укажите ровно один путь"
        );
    }

    #[test]
    fn flag_without_value_errors() {
        assert_eq!(
            err(parse_init_args(&args(&["--id"]))),
            "--id требует значение"
        );
        assert_eq!(
            err(parse_init_args(&args(&["--name"]))),
            "--name требует значение"
        );
    }

    
    
    #[test]
    fn transfer_rejects_second_id_with_command_prefix() {
        assert_eq!(
            err(parse_install_args(&args(&["one", "two"]))),
            "install: укажите ровно один id"
        );
        assert_eq!(
            err(parse_link_args(&args(&["one", "two"]))),
            "link: укажите ровно один id"
        );
        assert_eq!(
            err(parse_install_args(&args(&[]))),
            "install требует id <id>"
        );
        assert_eq!(err(parse_link_args(&args(&[]))), "link требует id <id>");
    }

    
    #[test]
    fn update_takes_optional_id_and_dest() {
        assert_eq!(
            parse_update_args(&args(&["--dest", "d"])).unwrap(),
            Action::Update {
                id: None,
                repo: None,
                dest: Some(PathBuf::from("d")),
            }
        );
        assert_eq!(
            parse_update_args(&args(&["word-count"])).unwrap(),
            Action::Update {
                id: Some("word-count".to_string()),
                repo: None,
                dest: None,
            }
        );
        assert_eq!(
            err(parse_update_args(&args(&["a", "b"]))),
            "update: укажите не более одного id"
        );
    }

    #[test]
    fn flags_of_every_command_parse_into_action() {
        assert_eq!(
            parse_test_args(&args(&["p", "--json", "--verbose"])).unwrap(),
            Action::Test {
                path: PathBuf::from("p"),
                json: true,
                verbose: true,
            }
        );
        assert_eq!(
            parse_init_args(&args(&["d", "--id", "i", "--name", "n", "--force"])).unwrap(),
            Action::Init {
                dir: PathBuf::from("d"),
                id: Some("i".to_string()),
                name: Some("n".to_string()),
                force: true,
            }
        );
        assert_eq!(
            parse_list_args(&args(&["--repo", "r", "--json"])).unwrap(),
            Action::List {
                repo: Some(PathBuf::from("r")),
                json: true,
            }
        );
        assert_eq!(
            parse_install_args(&args(&["i", "--repo", "r", "--dest", "d", "--force"])).unwrap(),
            Action::Install {
                id: "i".to_string(),
                repo: Some(PathBuf::from("r")),
                dest: Some(PathBuf::from("d")),
                force: true,
            }
        );
        assert_eq!(
            parse_link_args(&args(&["i", "--force"])).unwrap(),
            Action::Link {
                id: "i".to_string(),
                repo: None,
                dest: None,
                force: true,
            }
        );
        assert_eq!(
            parse_remove_args(&args(&["i", "--dest", "d"])).unwrap(),
            Action::Remove {
                id: "i".to_string(),
                dest: Some(PathBuf::from("d")),
            }
        );
    }

    
    
    #[test]
    fn unknown_flag_is_rejected_by_every_command() {
        type ParseFn = fn(&[String]) -> Result<Action, String>;
        let cases: [(&str, ParseFn); 7] = [
            ("test", parse_test_args),
            ("init", parse_init_args),
            ("list", parse_list_args),
            ("install", parse_install_args),
            ("link", parse_link_args),
            ("remove", parse_remove_args),
            ("update", parse_update_args),
        ];
        for (command, parse) in cases {
            assert_eq!(
                err(parse(&args(&["--nope"]))),
                "неизвестный аргумент: --nope",
                "{command}"
            );
        }
    }

    
    #[test]
    fn list_and_update_reject_extra_positional() {
        assert_eq!(
            err(parse_list_args(&args(&["лишний"]))),
            "лишний аргумент: лишний"
        );
    }
}
