use anyhow::{Context, Result};
use keyzen_core::{Lesson, LessonType};
use std::fs;
use std::path::{Path, PathBuf};

pub struct LessonLoader {
    lessons_dir: PathBuf,
}

impl LessonLoader {
    pub fn new(lessons_dir: impl Into<PathBuf>) -> Self {
        Self {
            lessons_dir: lessons_dir.into(),
        }
    }

    /// 加载所有课程
    pub fn load_all(&self) -> Result<Vec<Lesson>> {
        let mut lessons = Vec::new();

        self.load_from_dir_recursive(&self.lessons_dir, &mut lessons)?;

        lessons.sort_by_key(|l| l.id);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loader_creation() {
        let loader = LessonLoader::new("./lessons");
        assert!(loader.lessons_dir.ends_with("lessons"));
    }
}
