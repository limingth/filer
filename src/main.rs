// #![windows_subsystem = "windows"]

mod config;
mod context;
mod fileutil;
mod json_helper;

#[cfg(any(feature = "server", feature = "download"))]
mod base16;
#[cfg(feature = "download")]
mod download;
#[cfg(feature = "server")]
mod api;

#[cfg(feature = "server")]
mod static_files;

#[cfg(any(feature = "server", feature = "download"))]
mod addr;
#[cfg(feature = "xcopy")]
mod xcopy;
use std::fs::{self, File};
use std::path::{PathBuf, Path};
use std::io::{self, BufRead};
use byte_unit::Byte;


#[cfg(any(feature = "server", feature = "download"))]
use std::sync::Arc;
#[cfg(any(feature = "server", feature = "download"))]
use axum::{Router};

use tokio::sync::Mutex;
use once_cell::sync::Lazy;

pub static SERVER_HANDLE:Lazy<Mutex<Vec<axum_server::Handle>>> =
    Lazy::new(|| Mutex::new(Vec::new()));

use iced::{
    button, text_input, scrollable, Button,
    Column, Container, Element, Length, Row, Application,
    Settings, Text, TextInput, Command, Clipboard, Scrollable,
    Align
};


// use anyhow::{Result, Ok};
use clap::{Arg, ArgMatches};
use context::AppContext;
use json_helper::JsonHelper;
use tokio::time::Instant;
use serde_json::Value;

#[cfg(feature = "digest")]
use fileutil::refresh_dir_files_digest;

const VERSION: &str =env!("CARGO_PKG_VERSION");

type MyResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where P: AsRef<Path>, {
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

#[derive(Default)]
struct Gui {
    input: text_input::State,
    input_value: String,
    get_file_list_button: button::State,
    download_button: button::State,
    message_tip: String,
    file_list: Vec<FileInfo>,
    remote_file_getted: bool,
    is_downloading: bool,
    path: PathBuf,
    scrollable: scrollable::State,
}

struct FileInfo {
    path: String,
    size: String,
    // scrollable: scrollable::State,
}

#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    GetFileListButtonPressed,
    DownLoadButtonPressed,
    ServerStarted(bool),
    RemoteFileListGetted((bool, String)),
    DownloadCompleted(bool)
}


impl Application for Gui {
    type Message = Message;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Gui::default(),
            Command::none()
        )
    }

    fn title(&self) -> String {
        #[cfg(feature = "server-gui")]
        return String::from("Server");

        #[cfg(feature = "client-gui")]
        return String::from("Client");
    }

    fn update(&mut self, message: Message, _:&mut Clipboard) -> Command<Message> {
        match message {
            Message::InputChanged(value) => self.input_value = value,
            Message::ServerStarted(started) => {
                println!("ServerStarted: {started}");

                // 显示filelist.txt
                if started {
                    let mut path = if self.path.starts_with(".") {
                        // 相对路径
                        fs::canonicalize(PathBuf::from(&self.path)).unwrap()
                    } else { PathBuf::from(&self.path) };

                    path.extend(["filelist.txt"]);

                    println!("{path:?}");

                    match read_lines(path) {
                        Err(e) => self.message_tip = format!("Failed to read local filelist.txt: {e}"),
                        Ok(lines) => {
                            let mut totle = Vec::new();
                            let mut file_count: usize = 0;
                            let mut total_size: u128 = 0;
                            for line in lines {
                                if let Ok(line) = line {
                                    println!("{}", line);
                                    let parts:Vec<&str> = line.split(",").collect();

                                    let size = parts[1];
                                    let file_path = parts[2];
                                    let size = size.parse::<u128>().unwrap();
                                    total_size += size;
                                    let size = Byte::from_bytes(size).get_appropriate_unit(false);
                                    totle.push(FileInfo{path: file_path.to_owned(), size: format!("{size}")});
                                    file_count += 1;
                                }      
                            }
                            self.file_list = totle;
                            self.message_tip = format!("Total {} files with size {}.", file_count, Byte::from_bytes(total_size).get_appropriate_unit(false));
                        }
                    }
                } else {
                    self.message_tip = "Failed to start service".to_owned();
                }
            },
            Message::RemoteFileListGetted((geted, file_list)) => {
                if !geted {
                    println!("Failed to get remote filelist.txt");
                    self.message_tip = format!("Failed to get remote filelist.txt");
                    self.remote_file_getted = false;
                } else {
                    self.remote_file_getted = true;
                    let remote_file_list = download::parse_file_list(&file_list);
                    let file_count = remote_file_list.len();

                    let mut totle = Vec::new();

                    let total_size = remote_file_list.iter().map(|x| x.1).sum::<u64>();
    
                    for file_info in remote_file_list {
                        totle.push(FileInfo{path: file_info.2.to_owned(), size: format!("{}", Byte::from_bytes(file_info.1 as u128).get_appropriate_unit(false))});
                    }

                    self.file_list = totle;
                    self.message_tip = format!("Total {} files with size {}.", file_count, Byte::from_bytes(total_size as u128).get_appropriate_unit(false));
                }
            },
            Message::DownLoadButtonPressed => {
                if !self.remote_file_getted {
                    self.message_tip = "Please get file list first.".to_owned();
                    return Command::none();
                }

                if self.is_downloading {
                    return Command::none();
                }

                self.is_downloading = true;

                return Command::perform(
                    handle_start_download(),
                    |completed| {
                        println!("Download Completed");
                        Message::DownloadCompleted(completed)
                    }
                )
            }
            Message::DownloadCompleted(completed) => {
                if completed {
                    self.message_tip = "Download completed".to_owned();
                } else {
                    self.message_tip = "Download error".to_owned();
                }
                self.is_downloading = false;
            }
            Message::GetFileListButtonPressed => {
                println!("{}", self.input_value);

                #[cfg(feature = "client-gui")]
                {
                    let path = "./demo_sent";
                    // let path = if &self.input_value == "" { "./" } else { &self.input_value };

                    match fs::metadata(path) {
                        Ok(metadata) if metadata.is_dir() => {
                            println!("is dir: {path}");
                            self.message_tip = format!("Reading files in {path}......");
                            self.path = PathBuf::from(path);
                            return Command::perform(
                                handle_start_server(path.to_owned()),
                                |result| {
                                    match result {
                                        Ok(_) =>  Message::ServerStarted(true),
                                        _ =>  Message::ServerStarted(false)
                                    }
                                },
                            );
                        },
                        _ => {
                            self.message_tip = format!("{path} is not a dirctory.");
                            self.input_value = "".to_owned();
                            return Command::none()
                        }
                    };
                }

                #[cfg(feature = "server-gui")]
                {
                    // println!("server-gui")
                    self.message_tip = "Getting file list ...".to_owned();
                    self.file_list.clear();


                    let context = AppContext::new();
                    let catalog = "tcsoftV6";

                    return Command::perform(
                        get_remote_file_list(context.config.clone(), catalog),
                        |file_list| {
                            match file_list {
                                Ok(file_list) => Message::RemoteFileListGetted((true, file_list)),
                                Err(_) => Message::RemoteFileListGetted((false, String::new()))
                            }
                        },
                    );
                }

            }
        }

        Command::none()
    }

    fn view(&mut self) -> Element<Message> {
 
        let text_input = TextInput::new(
            &mut self.input,
            "Type file directory, default: ./",
            &self.input_value,
            Message::InputChanged,
        )
        .padding(10)
        .size(20);

        #[cfg(feature = "client-gui")]
        let button_text = "Get Local File List";

        #[cfg(feature = "server-gui")]
        let button_text = "Get Remote File List";

        let button = Button::new(&mut self.get_file_list_button, Text::new(button_text))
            .padding(10)
            .on_press(Message::GetFileListButtonPressed);
        
                
        let message_tip: Text = Text::new(&self.message_tip).into();


        let mut file_list_scrollable = Scrollable::new(&mut self.scrollable)
            .padding(10)
            .spacing(10)
            .scrollbar_margin(0)
            .scrollbar_width(6)
            .scroller_width(5)
            .width(Length::Fill)
            .height(Length::Fill);

        for file_info in &self.file_list {
            file_list_scrollable = file_list_scrollable.push(
                Row::new()
                    .push(
                        Column::new()
                            .push(
                                Text::new(file_info.path.clone())
                            )
                            .width(Length::Fill)
                            .align_items(Align::Start)
                        )
                    .push(
                        Column::new()
                        .push(
                            Text::new(file_info.size.clone())
                        )
                        .width(Length::Shrink)
                        .align_items(Align::End)
                    )
            )
        };

        // server端的下载按钮
        #[cfg(feature = "server-gui")]
        let footer = Column::with_children(vec![
            Button::new(&mut self.download_button, Text::new(if self.is_downloading {"Downloading"} else {"Download"}))
                .padding(10)
                .on_press(Message::DownLoadButtonPressed)
                .into(),
        ])
        .width(Length::Fill)
        .align_items(Align::End);
        // server端的进度条

        let content = Column::new()
            .spacing(20)
            .padding(20)
            // .max_width(600)
            .width(Length::Fill)
            // .align_items(Align::Center)
            .push(
                Row::new()
                    .spacing(10)
                    // .push(text_input)
                    .push(button)
                )
            .push(message_tip)
            .push(file_list_scrollable);
        
        #[cfg(feature = "server-gui")]
        let content = content.push(footer);

        Container::new(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            // .center_y()
            .into()
    }
}

async fn get_remote_file_list(config: Value, catalog: &str) -> MyResult<String> {
    println!("Start downloading filelist.txt");
    let client_config = &config["client"];
    let catalog = client_config["catalog"].string(catalog);
    let base_url = download::base_url(client_config);
    let (_, _, bytes) = download::get_full_of_file(&base_url, &catalog, "filelist.txt").await?;
    let remote_file_list: String = String::from_utf8(bytes).unwrap();

    println!("filelist.txt getted");
    Ok(remote_file_list)
}

async fn handle_start_download() -> bool {
    let context = AppContext::new();

    let cpus = num_cpus::get() as u64;
    let time_start = Instant::now();

    let catalog = "tcsoftV6";
    let res = download::download_files(
        &context.config,
        true,
        cpus * 4,
        catalog,
    )
    .await;

    match res {
        Ok(_) => {
            let pcpus = num_cpus::get_physical() as u64;
            println!(
                "Time taken: {}\nNumber of CPU cores: {}x{}",
                time_taken(time_start),
                pcpus,
                cpus / pcpus
            );
            return true;
        },
        Err(e) => {
            println!("Download Error: {e}");
            return false;
        }
    }
}

#[cfg(feature = "client-gui")]
async fn handle_start_server(path: String) -> MyResult<()> {
    

    println!("Recevied {path}");
    
    let context = AppContext::new();
    let cpus = num_cpus::get() as u64;
    // let time_start = Instant::now();

    // let catalog = args.value_of("catalog").unwrap_or("tcsoftV6");
    let catalog = "tcsoftV6";
    let config = context.config[catalog].clone();
    let part_size = config["part_size"].u64(102400u64);
    let max_tasks = config["max_tasks"].u64(cpus * 2);

    let show_repeat = true;

    refresh_dir_files_digest(&path, "filelist.txt", part_size, max_tasks, show_repeat).await?;

    tokio::spawn(async {
        println!("Shutdown start=========================================================================");
        // clear all prev server
        let mut handles = SERVER_HANDLE.lock().await;

        loop {
            if let Some(handle) = handles.pop() {
                println!("Shutdown prev server ...");
                handle.shutdown();
            } else {
                println!("Break Shutdown prev server loop");
                break;
            }
        }

        println!("Shutdown end ================================================================================")

    });

    // * Server Start ====================================

    tokio::spawn(async move {
        println!("Start server...");
        server(&context).await;
    });

    // let _ = tokio::join!(server_task);

    println!("Server started!");

    // * Server End ====================================
    Ok(())
}

// #[tokio::main]
// async fn main() -> Result<()> {
fn main() -> MyResult<()> {

    // * GUI Start ====================================

    println!("Before GUI");
    // let _ = Gui::run(Settings::default());
    let _ = Gui::run(Settings {
        default_font: Some(include_bytes!(
          "./hei.ttf"
        // "./fangzheng.ttf"
        // "./simhei.ttf"
        )),
        ..Settings::default()
      });

    println!("After GUI");

    // * GUI End ====================================

    // tracing_subscriber::fmt::init();

    // let args = args();
    // let context = if let Some(config_file) = args.value_of("config") {
    //     AppContext::from(config_file.into())
    // } else {
    //     AppContext::new()
    // };
    // let cpus = num_cpus::get() as u64;
    // let time_start = Instant::now();

    // #[cfg(feature = "digest")]
    // let show_repeat = args.is_present("repeat");
    // if args.is_present("digest") || show_repeat {
    //     let catalog = args.value_of("catalog").unwrap_or("tcsoftV6");
    //     let config = context.config[catalog].clone();
    //     let part_size = config["part_size"].u64(102400u64);
    //     let max_tasks = config["max_tasks"].u64(cpus * 2);
    //     let path = config["path"].str("./demo_sent");
    //     refresh_dir_files_digest(path, "filelist.txt", part_size, max_tasks, show_repeat).await?;
    // }
    // #[cfg(feature = "xcopy")]
    // if args.is_present("xcopy") {
    //     let config = context.config.clone();
    //     let source_path = args.value_of("source_path").unwrap_or("");
    //     let target_path = args.value_of("target_path").unwrap_or("");
    //     if source_path.is_empty() || target_path.is_empty() {
    //         println!("Usage: filer --xcopy source_path target_path")
    //     } else {
    //         xcopy::xcopy_files(&config, source_path, target_path, cpus * 2).await?;
    //     }
    // }
    // if args.is_present("server") {
    //     println!();
    //     #[cfg(feature = "server")]
        // server(&context).await;
    // } else if args.is_present("download") || args.is_present("update") {
    //     let catalog = args.value_of("catalog").unwrap_or("tcsoftV6");
    //     #[cfg(feature = "download")]
    //     download::download_files(
    //         &context.config,
    //         args.is_present("download"),
    //         cpus * 4,
    //         catalog,
    //     )
    //     .await?;
    //     println!();
    // }
    // let pcpus = num_cpus::get_physical() as u64;
    // println!(
    //     "Time taken: {}\nNumber of CPU cores: {}x{}",
    //     time_taken(time_start),
    //     pcpus,
    //     cpus / pcpus
    // );
    Ok(())
}

#[cfg(feature = "server")]
async fn server(context: &Arc<AppContext>) {
    let server_config = context.config["server"].clone();

    let static_path = server_config["static_path"].string("public");
    let cache_age_in_minute: i32 = server_config["static_cache_age_in_minute"].i64(30) as i32;

    let ctx = context.clone();
    let app = Router::new()
        .nest("/api", api::api(ctx))
        .fallback(static_files::make_service(static_path, cache_age_in_minute));

    let http_server = tokio::spawn(start_server(server_config.clone(), false, app.clone()));
    let https_server = tokio::spawn(start_server(server_config, true, app));
    let (_, _) = tokio::join!(http_server, https_server);

}

#[cfg(feature = "server")]
async fn start_server(config: Value, is_https: bool, app: Router) {
    use axum_server::tls_rustls::RustlsConfig;
    use chrono::Local;
    use std::net::SocketAddr;
    let server_name = config["server_name"].string("W3");
    let protocol = if is_https { "HTTPS" } else { "HTTP" };
    let config_addr = addr::Addr::new(&config, is_https);
    let (is_active, addr) = config_addr.get();
    if is_active {
        let now = &Local::now().to_string()[0..19];
        println!(
            "{} {} server version {} started at {} listening on {}",
            server_name, protocol, VERSION, now, &config_addr
        );
        let app = app.into_make_service_with_connect_info::<SocketAddr>();
        let server = if is_https {
            let tls_config = RustlsConfig::from_pem_file("server.cer", "server.key")
                .await
                .unwrap();
            let handle = axum_server::Handle::new();
            SERVER_HANDLE.lock().await.push(handle.clone());

            axum_server::bind_rustls(addr, tls_config)
                .handle(handle)
                .serve(app).await
        } else {
            let handle = axum_server::Handle::new();
            SERVER_HANDLE.lock().await.push(handle.clone());

            axum_server::bind(addr)
            .handle(handle)
            .serve(app).await
        };
        server.unwrap();
    } else {
        println!(
            "{} {} server version {} is not active !",
            server_name, protocol, VERSION
        );
    }
}

// fn args() -> ArgMatches {
//     let app = clap::Command::new("Filer 文件传输系统")
//         .version(VERSION)
//         .author("xander.xiao@gmail.com")
//         .about("极速文件分发、拷贝工具")
//         .mut_arg("version", |a| a.help(Some("显示版本号")))
//         .mut_arg("help", |a| a.help(Some("显示帮助信息")))
//         .arg(
//             Arg::new("config")
//                 .help("指定配置文件")
//                 .short('C')
//                 .long("config")
//                 .value_name("config")
//                 .takes_value(true)
//                 .default_value("filer.json"),
//         );

//     #[cfg(any(feature = "server", feature = "calc_digest", feature = "download"))]
//     let app = app.arg(
//         Arg::new("catalog")
//             .help("指定分发目录")
//             .short('c')
//             .long("catalog")
//             .value_name("catalog")
//             .takes_value(true)
//             .default_value("tcsoftV6"),
//     );

//     #[cfg(feature = "digest")]
//     let app = app.arg(
//         Arg::new("digest")
//             .help("刷新文件列表，计算文件的哈希值")
//             .short('i')
//             .long("index"),
//     );

//     #[cfg(feature = "digest")]
//     let app = app.arg(
//         Arg::new("repeat")
//             .help("刷新文件哈希值列表时，列出重复文件")
//             .short('r')
//             .long("repeat"),
//     );

//     #[cfg(feature = "xcopy")]
//     let app = app
//         .arg(
//             Arg::new("xcopy")
//                 .help("复制文件夹或文件")
//                 .short('x')
//                 .long("xcopy"),
//         )
//         .arg(
//             Arg::new("source_path")
//                 .help("Sets the XCopy source path or file")
//                 .index(1),
//         )
//         .arg(
//             Arg::new("target_path")
//                 .help("Sets the XCopy target path")
//                 .index(2),
//         );

//     #[cfg(feature = "server")]
//     let app = app.arg(
//         Arg::new("server")
//             .help("作为服务器启动文件服务")
//             .short('s')
//             .long("server")
//             .conflicts_with("download")
//             .conflicts_with("update"),
//     );

//     #[cfg(feature = "download")]
//     let app = app
//         .arg(
//             Arg::new("download")
//                 .help("作为客户端下载所有文件")
//                 .short('d')
//                 .long("download")
//                 .conflicts_with("server")
//                 .conflicts_with("update"),
//         )
//         .arg(
//             Arg::new("update")
//                 .help("作为客户端下载更新文件")
//                 .short('u')
//                 .long("update")
//                 .conflicts_with("server")
//                 .conflicts_with("download"),
//         );
//     app.get_matches()
// }

fn time_taken(start_time: Instant) -> String {
    let dur = Instant::now() - start_time;
    let dur: f32 = dur.as_secs_f32();
    const F60: f32 = 60f32;
    if dur > F60 * F60 {
        let h = (dur / (F60 * F60)).round();
        let m = ((dur - h * F60 * F60) / F60).round();
        let s = dur - m * F60;
        format!("{}h{}m{:.2}s", h as i32, m as i32, s)
    } else if dur > F60 {
        let m = (dur / F60).round();
        let s = dur - m * F60;
        format!("{}m{:.2}s", m as i32, s)
    } else {
        format!("{:.2}s", dur)
    }
}
