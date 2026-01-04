use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use log::info;

/// 远程课程元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteLessonMeta {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub author: String,
    pub difficulty: String,
    pub download_url: String,
    pub version: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// 课程商店 API 响应
#[derive(Debug, Deserialize)]
pub struct LessonStoreResponse {
    pub lessons: Vec<RemoteLessonMeta>,
}

/// 课程商店管理器
#[derive(Clone)]
pub struct LessonStoreManager {
    api_url: String,
    user_data_dir: PathBuf,
    client: reqwest::blocking::Client,
}

impl LessonStoreManager {
    pub fn new(api_url: String, user_data_dir: PathBuf) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        Self {
            api_url,
            user_data_dir,
            client,
        }
    }

    /// 获取远程课程列表
    pub fn fetch_lessons(&self) -> Result<Vec<RemoteLessonMeta>> {
        info!("正在从 {} 获取课程列表...", self.api_url);

        let response = self.client
            .get(&self.api_url)
            .send()
            .context("无法连接到课程服务器")?;

        let store_response: LessonStoreResponse = response
            .json()
            .context("解析课程列表失败")?;

        info!("成功获取 {} 个课程", store_response.lessons.len());
        Ok(store_response.lessons)
    }

    /// 下载课程到本地
    pub fn download_lesson(&self, lesson: &RemoteLessonMeta) -> Result<PathBuf> {
        info!("正在下载课程: {} (ID: {})", lesson.title, lesson.id);

        let response = self.client
            .get(&lesson.download_url)
            .send()
            .context("下载课程文件失败")?;

        let ron_content = response
            .text()
            .context("读取课程内容失败")?;

        // 保存到用户数据目录
        let file_path = self.user_data_dir.join(format!("{}.ron", lesson.id));
        std::fs::write(&file_path, ron_content)
            .context("保存课程文件失败")?;

        info!("课程已保存到: {:?}", file_path);
        Ok(file_path)
    }

    /// 检查课程是否已下载（通过 ID）
    pub fn is_downloaded(&self, lesson_id: u32, local_lesson_ids: &[u32]) -> bool {
        local_lesson_ids.contains(&lesson_id)
    }

    /// 删除已下载的课程
    pub fn delete_lesson(&self, lesson_id: u32) -> Result<()> {
        let file_path = self.user_data_dir.join(format!("{}.ron", lesson_id));

        if file_path.exists() {
            std::fs::remove_file(&file_path)
                .context(format!("删除课程文件失败: {:?}", file_path))?;
            info!("已删除课程文件: {:?}", file_path);
            Ok(())
        } else {
            anyhow::bail!("课程文件不存在: {:?}", file_path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_lesson_meta_deserialize() {
        let json = r#"{
            "id": 1001,
            "title": "测试课程",
            "description": "这是一个测试",
            "author": "测试作者",
            "difficulty": "初级",
            "download_url": "https://example.com/1001.ron",
            "version": "1.0.0",
            "tags": ["测试"]
        }"#;

        let meta: RemoteLessonMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.id, 1001);
        assert_eq!(meta.title, "测试课程");
    }
}
