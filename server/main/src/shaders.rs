use std::{
    collections::{HashMap, HashSet, LinkedList},
    path::{PathBuf},
    io::{BufReader, BufRead},
};

use path_slash::PathBufExt;
use regex::Regex;

use lazy_static::lazy_static;
use slog_scope::info;

lazy_static! {
    static ref RE_MACRO_INCLUDE: Regex = Regex::new(r#"^(?:\s)*?(?:#include) "(.+)"\r?"#).unwrap();
}

pub struct ShaderFile {
    path: PathBuf,
    work_space: PathBuf,
    including_file: LinkedList<(usize, PathBuf)>,
}

impl ShaderFile {
    pub fn including_file(&self) -> &LinkedList<(usize, PathBuf)> {
        &self.including_file
    }

    pub fn read_file (&mut self, include_files: &mut HashMap<PathBuf, IncludeFile>) {
        let shader_path = self.path.as_path();
        let reader = BufReader::new(std::fs::File::open(shader_path).unwrap());
        reader.lines()
            .enumerate()
            .filter_map(|line| match line.1 {
                Ok(t) => Some((line.0, t)),
                Err(_e) => None,
            })
            .filter(|line| RE_MACRO_INCLUDE.is_match(line.1.as_str()))
            .for_each(|line| {
                let cap = RE_MACRO_INCLUDE.captures(line.1.as_str()).unwrap().get(1).unwrap();
                let path: String = cap.as_str().into();

                let include_path = if path.starts_with('/') {
                    let path = path.strip_prefix('/').unwrap().to_string();
                    self.work_space.join(PathBuf::from_slash(&path))
                } else {
                    shader_path.parent().unwrap().join(PathBuf::from_slash(&path))
                };

                self.including_file.push_back((line.0, include_path.clone()));

                IncludeFile::get_includes(&self.work_space, &include_path, &self.path, include_files, 0);
            });
    }

    pub fn new(work_space: &PathBuf, path: &PathBuf) -> ShaderFile {
        ShaderFile {
            path: path.clone(),
            work_space: work_space.clone(),
            including_file: LinkedList::new(),
        }
    }
}

#[derive(Clone)]
pub struct IncludeFile {
    path: PathBuf,
    work_space: PathBuf,
    included_file: HashSet<PathBuf>,
    including_file: LinkedList<(usize, PathBuf)>,
}

impl IncludeFile {
    pub fn included_file(&self) -> &HashSet<PathBuf> {
        &self.included_file
    }

    pub fn including_file(&self) -> &LinkedList<(usize, PathBuf)> {
        &self.including_file
    }

    pub fn update_parent(&mut self, parent_file: &PathBuf, include_files: &mut HashMap<PathBuf, IncludeFile>, depth: i32) {
        if depth > 10 {
            return;
        }

        self.included_file.insert(parent_file.clone());
        
        for file in &self.including_file {
            let mut sub_include_file = include_files.remove(&file.1).unwrap();
            sub_include_file.update_parent(parent_file, include_files, depth + 1);
            include_files.insert(file.1.clone(), sub_include_file);
        }
    }

    pub fn get_includes(work_space: &PathBuf, include_path: &PathBuf, parent_file: &PathBuf, include_files: &mut HashMap<PathBuf, IncludeFile>, depth: i32) {
        if depth > 10 {
            return;
        }
        if include_files.contains_key(include_path) {
            let mut include = include_files.remove(include_path).unwrap();
            include.included_file.insert(parent_file.clone());
            for file in &include.including_file {
                let mut sub_include_file = include_files.remove(&file.1).unwrap();
                sub_include_file.update_parent(parent_file, include_files, depth + 1);
                include_files.insert(file.1.clone(), sub_include_file);
            }
            include_files.insert(include_path.clone(), include);
        }
        else {
            let mut include = IncludeFile {
                path: include_path.clone(),
                work_space: work_space.clone(),
                included_file: HashSet::new(),
                including_file: LinkedList::new(),
            };
            include.included_file.insert(parent_file.clone());

            // info!("found include file : {}", include_path.to_str().unwrap());

            let reader = BufReader::new(std::fs::File::open(include_path).unwrap());
            reader.lines()
                .enumerate()
                .filter_map(|line| match line.1 {
                    Ok(t) => Some((line.0, t)),
                    Err(_e) => None,
                })
                .filter(|line| RE_MACRO_INCLUDE.is_match(line.1.as_str()))
                .for_each(|line| {
                    let cap = RE_MACRO_INCLUDE.captures(line.1.as_str()).unwrap().get(1).unwrap();
                    let path: String = cap.as_str().into();

                    let sub_include_path = if path.starts_with('/') {
                        let path = path.strip_prefix('/').unwrap().to_string();
                        work_space.join(PathBuf::from_slash(&path))
                    } else {
                        include_path.parent().unwrap().join(PathBuf::from_slash(&path))
                    };

                    include.including_file.push_back((line.0, sub_include_path.clone()));

                    Self::get_includes(work_space, &sub_include_path, parent_file, include_files, depth + 1);
                });

            include_files.insert(include_path.clone(), include);
        }
    }
}
