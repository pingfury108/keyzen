use anyhow::{Context, Result};
use keyzen_core::{Lesson, LessonType};
use log::debug;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use rust_embed::RustEmbed;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

/// 嵌入式课程资源
#[derive(RustEmbed)]
#[folder = "../../lessons"]
#[include = "*.ron"]
struct EmbeddedLessons;

pub struct LessonLoader {
    user_data_dir: PathBuf,
    watcher: Option<RecommendedWatcher>,
}

impl LessonLoader {
    pub fn new(_lessons_dir: impl Into<PathBuf>) -> Result<Self> {
        let user_data_dir = Self::get_user_data_dir()?;

        // 确保用户数据目录存在
        if !user_data_dir.exists() {
            fs::create_dir_all(&user_data_dir)
                .with_context(|| format!("Failed to create user data dir: {:?}", user_data_dir))?;
            debug!("✅ 创建用户数据目录: {:?}", user_data_dir);
        }

        Ok(Self {
            user_data_dir,
            watcher: None,
        })
    }

    /// 获取系统数据目录
    fn get_user_data_dir() -> Result<PathBuf> {
        #[cfg(target_os = "macos")]
        let base = dirs::data_local_dir().ok_or_else(|| anyhow::anyhow!("无法获取数据目录"))?;

        #[cfg(target_os = "linux")]
        let base = dirs::data_local_dir().ok_or_else(|| anyhow::anyhow!("无法获取数据目录"))?;

        #[cfg(target_os = "windows")]
        let base = dirs::data_dir().ok_or_else(|| anyhow::anyhow!("无法获取数据目录"))?;

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        let base = PathBuf::from(".");

        Ok(base.join("Keyzen").join("lessons"))
    }

    /// 加载所有课程（用户目录覆盖内置资源）
    pub fn load_all(&self) -> Result<Vec<Lesson>> {
        let mut lessons_map: HashMap<u32, Lesson> = HashMap::new();

        // 1. 先加载嵌入式内置课程
        let builtin = self.load_embedded_lessons()?;
        debug!("📚 加载嵌入式课程: {} 个", builtin.len());
        for lesson in builtin {
            lessons_map.insert(lesson.id, lesson);
        }

        // 2. 再加载用户课程（覆盖同 ID 的内置课程）
        let mut user_lessons = Vec::new();
        self.load_from_dir_recursive(&self.user_data_dir, &mut user_lessons)?;
        if !user_lessons.is_empty() {
            debug!("📚 加载用户课程: {} 个", user_lessons.len());
        }
        for lesson in user_lessons {
            if lessons_map.contains_key(&lesson.id) {
                debug!("🔄 用户课程覆盖内置课程 ID: {}", lesson.id);
            }
            lessons_map.insert(lesson.id, lesson);
        }

        // 3. 排序返回
        let mut lessons: Vec<_> = lessons_map.into_values().collect();
        lessons.sort_by_key(|l| l.id);
        Ok(lessons)
    }

    /// 从嵌入式资源加载课程
    fn load_embedded_lessons(&self) -> Result<Vec<Lesson>> {
        let mut lessons = Vec::new();

        for file in EmbeddedLessons::iter() {
            let file_name = file.as_ref();

            // 只处理 .ron 文件
            if !file_name.ends_with(".ron") {
                continue;
            }

            if let Some(content) = EmbeddedLessons::get(file_name) {
                let content_str = std::str::from_utf8(&content.data)
                    .with_context(|| format!("Failed to decode embedded file: {}", file_name))?;

                let lesson: Lesson = ron::from_str(content_str)
                    .with_context(|| format!("Failed to parse embedded lesson: {}", file_name))?;

                lessons.push(lesson);
            }
        }

        Ok(lessons)
    }

    /// 递归加载目录中的所有课程
    fn load_from_dir_recursive(&self, dir: &Path, lessons: &mut Vec<Lesson>) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        for entry in
            fs::read_dir(dir).with_context(|| format!("Failed to read directory: {:?}", dir))?
        {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                self.load_from_dir_recursive(&path, lessons)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("ron") {
                let content = fs::read_to_string(&path)?;
                let lesson: Lesson = ron::from_str(&content)
                    .with_context(|| format!("Failed to parse: {:?}", path))?;
                lessons.push(lesson);
            }
        }

        Ok(())
    }

    /// 按 ID 加载单个课程
    pub fn load_by_id(&self, id: u32) -> Result<Lesson> {
        let all_lessons = self.load_all()?;
        all_lessons
            .into_iter()
            .find(|l| l.id == id)
            .ok_or_else(|| anyhow::anyhow!("Lesson with id {} not found", id))
    }

    /// 按类型加载课程
    pub fn load_by_type(&self, lesson_type: LessonType) -> Result<Vec<Lesson>> {
        let all_lessons = self.load_all()?;
        Ok(all_lessons
            .into_iter()
            .filter(|l| l.lesson_type == lesson_type)
            .collect())
    }

    /// 按语言加载课程
    pub fn load_by_language(&self, language: &str) -> Result<Vec<Lesson>> {
        let all_lessons = self.load_all()?;
        Ok(all_lessons
            .into_iter()
            .filter(|l| l.language == language)
            .collect())
    }

    /// 启动文件系统监听，自动检测课程变化（仅监听用户数据目录）
    pub fn start_watching<F>(&mut self, callback: F) -> Result<()>
    where
        F: Fn() + Send + 'static,
    {
        let (tx, rx) = channel();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                // 只关心 .ron 文件的变化
                let has_ron_change = event
                    .paths
                    .iter()
                    .any(|p| p.extension().and_then(|s| s.to_str()) == Some("ron"));

                if has_ron_change {
                    debug!("📂 检测到用户课程文件变化: {:?}", event.paths);
                    tx.send(()).ok();
                }
            }
        })?;

        // 只监听用户数据目录（内置目录编译到二进制，无需监听）
        watcher.watch(&self.user_data_dir, RecursiveMode::Recursive)?;

        debug!("👀 开始监听用户课程目录: {:?}", self.user_data_dir);

        // 启动监听线程
        std::thread::spawn(move || {
            while rx.recv().is_ok() {
                callback();
            }
        });

        self.watcher = Some(watcher);
        Ok(())
    }

    /// 获取用户数据目录路径（供外部使用）
    pub fn get_user_data_dir_path() -> Result<PathBuf> {
        Self::get_user_data_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loader_creation() {
        let loader = LessonLoader::new("./lessons").expect("Failed to create loader");
        assert!(loader.user_data_dir.to_string_lossy().contains("Keyzen"));
    }

    #[test]
    fn test_embedded_lessons_load() {
        let loader = LessonLoader::new("./lessons").expect("Failed to create loader");
        let lessons = loader
            .load_embedded_lessons()
            .expect("Failed to load embedded lessons");
        assert!(!lessons.is_empty(), "Should load embedded lessons");
    }
}
