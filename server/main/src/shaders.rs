use std::{
    collections::{HashMap, HashSet, LinkedList},
    path::{PathBuf},
    io::{BufReader, BufRead},
};

use path_slash::PathBufExt;
use regex::Regex;

use lazy_static::lazy_static;
use slog_scope::{info, error};

lazy_static! {
    static ref RE_MACRO_INCLUDE: Regex = Regex::new(r#"^(?:\s)*?(?:#include) "(.+)"\r?"#).unwrap();
}

pub struct ShaderFile {
    // File path
    path: PathBuf,
    // The work space that this file in
    work_space: PathBuf,
    // Files included in this file (line, file path)
    including_files: LinkedList<(usize, PathBuf)>,
}

impl ShaderFile {
    pub fn including_files(&self) -> &LinkedList<(usize, PathBuf)> {
        &self.including_files
    }

    pub fn clear_including_files(&mut self) {
        self.including_files.clear();
    }

    pub fn new(work_space: &PathBuf, path: &PathBuf) -> ShaderFile {
        ShaderFile {
            path: path.clone(),
            work_space: work_space.clone(),
            including_files: LinkedList::new(),
        }
    }

    pub fn read_file (&mut self, include_files: &mut HashMap<PathBuf, IncludeFile>) {
        let shader_path = self.path.as_path();

        let mut parent_path: HashSet<PathBuf> = HashSet::new();
        parent_path.insert(self.path.clone());

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

                self.including_files.push_back((line.0, include_path.clone()));

                IncludeFile::get_includes(&self.work_space, &include_path, &parent_path, include_files, 0);
            });
    }

    pub fn merge_shader_file(&self, include_files: &HashMap<PathBuf, IncludeFile>, file_list: &mut HashMap<i32, PathBuf>) -> String {
        let mut shader_content: String = String::new();
        file_list.insert(0, self.path.clone());

        let mut including_files = self.including_files.clone();
        let mut next_include_file = match including_files.pop_front() {
            Some(include_file) => include_file,
            None => (usize::from(u16::MAX), PathBuf::from("/")),
        };
        let mut file_id = 0;

        let shader_reader = BufReader::new(std::fs::File::open(self.path.clone()).unwrap());
        shader_reader.lines()
            .enumerate()
            .filter_map(|line| match line.1 {
                Ok(t) => Some((line.0, t)),
                Err(_e) => None,
            })
            .for_each(|line| {
                if line.0 == next_include_file.0 {
                    let include_file = include_files.get(&next_include_file.1).unwrap();
                    file_id += 1;
                    let include_content = include_file.merge_include(&line.1, include_files, file_list, &mut file_id);
                    shader_content += &include_content;
                    next_include_file = match including_files.pop_front() {
                        Some(include_file) => include_file,
                        None => (usize::from(u16::MAX), PathBuf::from("/")),
                    };
                    shader_content += &format!("#line {} 0\n", line.0 + 2);
                }
                else {
                    shader_content += &line.1;
                    shader_content += "\n";
                }
            });
        shader_content
    }
}

#[derive(Clone)]
pub struct IncludeFile {
    // File path
    path: PathBuf,
    // The work space that this file in
    work_space: PathBuf,
    // Shader files that include this file
    included_shaders: HashSet<PathBuf>,
    // Files included in this file (line, file path)
    including_files: LinkedList<(usize, PathBuf)>,
}

impl IncludeFile {
    pub fn included_shaders(&self) -> &HashSet<PathBuf> {
        &self.included_shaders
    }

    pub fn including_files(&self) -> &LinkedList<(usize, PathBuf)> {
        &self.including_files
    }

    pub fn update_parent(include_path: &PathBuf, parent_file: &HashSet<PathBuf>, include_files: &mut HashMap<PathBuf, IncludeFile>, depth: i32) {
        if depth > 10 {
            return;
        }
        let mut include_file = include_files.remove(include_path).unwrap();
        include_file.included_shaders.extend(parent_file.clone());
        include_files.insert(include_path.clone(), include_file.clone());
        
        for file in &include_file.including_files {
            Self::update_parent(&file.1, parent_file, include_files, depth + 1);
        }
    }

    pub fn get_includes(work_space: &PathBuf, include_path: &PathBuf, parent_file: &HashSet<PathBuf>, include_files: &mut HashMap<PathBuf, IncludeFile>, depth: i32) {
        if depth > 10 {
            return;
        }
        if include_files.contains_key(include_path) {
            let mut include = include_files.remove(include_path).unwrap();
            include.included_shaders.extend(parent_file.clone());
            for file in &include.including_files {
                Self::update_parent(&file.1, parent_file, include_files, depth + 1);
            }
            include_files.insert(include_path.clone(), include);
        }
        else {
            let mut include = IncludeFile {
                path: include_path.clone(),
                work_space: work_space.clone(),
                included_shaders: HashSet::new(),
                including_files: LinkedList::new(),
            };
            include.included_shaders.extend(parent_file.clone());

            if include_path.exists() {
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

                        include.including_files.push_back((line.0, sub_include_path.clone()));

                        Self::get_includes(work_space, &sub_include_path, parent_file, include_files, depth + 1);
                    });
            }
            else {
                error!("cannot find include file {}", include_path.to_str().unwrap());
            }

            include_files.insert(include_path.clone(), include);
        }
    }

    pub fn update_include(&mut self, include_files: &mut HashMap<PathBuf, IncludeFile>) {
        self.including_files.clear();

        let reader = BufReader::new(std::fs::File::open(self.path.as_path()).unwrap());
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
                    self.work_space.join(PathBuf::from_slash(&path))
                } else {
                    self.path.parent().unwrap().join(PathBuf::from_slash(&path))
                };

                self.including_files.push_back((line.0, sub_include_path.clone()));

                Self::get_includes(&self.work_space, &sub_include_path, &self.included_shaders, include_files, 1);
            });
    }

    pub fn merge_include(&self, original_content: &String, include_files: &HashMap<PathBuf, IncludeFile>, file_list: &mut HashMap<i32, PathBuf>, file_id: &mut i32) -> String {
        if (!self.path.exists()) {
            original_content.clone() + "\n"
        }
        else {
            let mut include_content: String = String::new();
            file_list.insert(file_id.clone(), self.path.clone());
            include_content += &format!("#line 1 {}\n", &file_id.to_string());

            let curr_file_id = file_id.clone();
            let mut including_files = self.including_files.clone();
            let mut next_include_file = match including_files.pop_front() {
                Some(include_file) => include_file,
                None => (usize::from(u16::MAX), PathBuf::from("/")),
            };

            let shader_reader = BufReader::new(std::fs::File::open(self.path.clone()).unwrap());
            shader_reader.lines()
                .enumerate()
                .filter_map(|line| match line.1 {
                    Ok(t) => Some((line.0, t)),
                    Err(_e) => None,
                })
                .for_each(|line| {
                    if line.0 == next_include_file.0 {
                        let include_file = include_files.get(&next_include_file.1).unwrap();
                        *file_id += 1;
                        let sub_include_content = include_file.merge_include(&line.1, include_files, file_list, file_id);
                        include_content += &sub_include_content;
                        next_include_file = match including_files.pop_front() {
                            Some(include_file) => include_file,
                            None => (usize::from(u16::MAX), PathBuf::from("/")),
                        };
                        include_content += &format!("#line {} {}\n", line.0 + 2, curr_file_id);
                    }
                    else {
                        include_content += &line.1;
                        include_content += "\n";
                    }
                });
            include_content
        }
    }
}
