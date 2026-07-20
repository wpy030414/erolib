

mod collection_service;
mod feed;
mod library;
pub mod locale;
mod opds;
mod rss;
pub mod pixiv;
pub mod ehentai;
pub mod ahentai;
pub mod nicecat;
mod search;
pub mod similarity;
mod storage;
pub mod aria2;
pub mod proxy;
pub mod task;
pub mod task_manager;

pub use aria2::Aria2Client;
pub use collection_service::CollectionService;
pub use library::LibraryService;
pub use opds::OpdsService;
pub use rss::RssService;
pub use ehentai::{EhentaiClient, GalleryListItem};
pub use ahentai::AhentaiClient;
pub use nicecat::NicecatClient;
pub use search::SearchService;
pub use storage::StorageService;
