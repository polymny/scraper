//! This module helps us run python cropper.

use std::process::Stdio;

use serde::{Deserialize, Serialize};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

use crate::config::Config;
use crate::db::Media;
use crate::{Db, Error, Result};

/// A message that can be sent to python.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Add a file to the current batch.
    AddFile(AddFileRequest),

    /// Ask to run the current batch.
    Run,

    /// Terminates the cropper.
    End,
}

impl Request {
    /// Returns true if we should expect results from python.
    ///
    /// True for Run and End.
    pub fn should_wait_python(&self) -> bool {
        match self {
            Request::AddFile(_) => false,
            _ => true,
        }
    }
}

/// The different elements needed to ask python to crop a file.
#[derive(Serialize, Deserialize)]
pub struct AddFileRequest {
    /// The id of the media in the database.
    pub id: i32,

    /// Path to the file.
    pub path: String,
}

/// A message that was received by to python.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// A file was cropped.
    FileCropSuccess(FileCropSuccessResponse),

    /// A file cropping has failed.
    FileCropFailure(FileCropFailureResponse),
}

/// A file was cropped.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileCropSuccessResponse {
    /// The id of the media.
    pub id: i32,

    /// The path to the file.
    pub path: String,

    /// The x coordinate of the center of the bounding box.
    pub x: f64,

    /// The y coordinate of the center of the bounding box.
    pub y: f64,

    /// The width of the bounding box.
    pub width: f64,

    /// The height of the bounding box.
    pub height: f64,

    /// The confidence given by the network.
    pub confidence: f64,
}

/// A file couldn't be cropped.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileCropFailureResponse {
    /// The id of the media.
    pub id: i32,

    /// The path to the file.
    pub path: String,
}

/// This structure holds the cropper python command.
pub struct Cropper {
    /// An access to the db.
    pub db: Db,

    /// An access to the config file.
    pub config: Config,

    /// The python command's stdin.
    pub stdin: ChildStdin,

    /// The python command's stdout.
    pub stdout: BufReader<ChildStdout>,

    /// The number of files in the batch.
    pub batch_size: usize,

    /// The number of files to put in each batch.
    pub batch_capacity: usize,
}

impl Cropper {
    /// Creates a new cropper.
    pub fn new(batch_capacity: usize, config: Config, db: Db) -> Result<Cropper> {
        let mut command = Command::new("python");
        command.arg("python/main.py");
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());

        let mut child = command.spawn()?;
        let stdin = child.stdin.take().ok_or(Error::InitializeCropperFailed)?;
        let stdout = child.stdout.take().ok_or(Error::InitializeCropperFailed)?;

        Ok(Cropper {
            stdin,
            stdout: BufReader::new(stdout),
            config,
            db,
            batch_size: 0,
            batch_capacity,
        })
    }

    /// Asks python to crop a media, and trigger if batch size is reached..
    pub async fn add_media(&mut self, media: &Media) -> Result<()> {
        if let Some(path) = &media.path {
            self.send_request(Request::AddFile(AddFileRequest {
                id: media.id,
                path: format!(
                    "{}/medias/{}",
                    self.config
                        .storage
                        .data_path
                        .to_str()
                        .expect("Failed to convert path to str"),
                    path.clone()
                ),
            }))
            .await?;
        } else {
            error!("Asked python to scrap non downloaded media");
            return Ok(());
        }

        self.batch_size += 1;

        if self.batch_size >= self.batch_capacity {
            info!("Batch full: asking for python to run cropping");
            self.send_request(Request::Run).await?;
            self.batch_size = 0;
        }

        Ok(())
    }

    /// Sends a json request to the python.
    pub async fn send_request(&mut self, request: Request) -> Result<()> {
        self.stdin
            .write(format!("{}\n", serde_json::to_string(&request)?).as_bytes())
            .await?;

        if request.should_wait_python() {
            self.stdin.flush().await?;
            self.wait_python().await?;
        }

        Ok(())
    }

    /// Waits for the python response.
    pub async fn wait_python(&mut self) -> Result<()> {
        let mut line = String::new();
        let bytes = self.stdout.read_line(&mut line).await?;

        if bytes == 0 {
            info!("Received EOF from python");
            return Ok(());
        }

        info!("Received response from python");

        let responses: Vec<Response> = serde_json::from_str(&line)?;
        let mut t = self.db.transaction().await?;
        let mut failures = vec![];

        for response in &responses {
            match response {
                Response::FileCropSuccess(file_crop_success) => {
                    let mut media = Media::get_by_id(file_crop_success.id, &mut t)
                        .await?
                        .expect("Python answered with media id that doesn't exists, this should never happen");

                    media.cropped = true;
                    media.x = Some(file_crop_success.x);
                    media.y = Some(file_crop_success.y);
                    media.width = Some(file_crop_success.width);
                    media.height = Some(file_crop_success.height);
                    media.confidence = Some(file_crop_success.confidence);
                    media.save(&mut t).await?;
                }

                Response::FileCropFailure(file_crop_failure) => {
                    failures.push(format!("{}", file_crop_failure.id));
                    let mut media = Media::get_by_id(file_crop_failure.id, &mut t)
                        .await?
                        .expect("Python answered with media id that doesn't exists, this should never happen");

                    media.cropped = true;
                    media.save(&mut t).await?;
                }
            }
        }

        t.commit().await?;

        if failures.len() > 0 {
            warn!(
                "Python failed to crop {}: {}",
                if failures.len() > 1 {
                    "a few files"
                } else {
                    "a file"
                },
                failures.join(", ")
            )
        }
        info!(
            "Python successfully cropped {} out of {} images",
            responses.len() - failures.len(),
            responses.len()
        );

        Ok(())
    }

    /// Starts a thread that runs wait_python by itself.
    ///
    /// It will receive the ids of the medias to crop via the mscp channel.
    /// None means that we need to crop the remaining files and exit.
    pub fn run(self, receiver: UnboundedReceiver<Option<i32>>) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut cropper = self;
            let mut receiver = receiver;
            loop {
                match receiver.recv().await {
                    Some(Some(id)) => {
                        let db = std::ops::DerefMut::deref_mut(&mut cropper.db.0);
                        match Media::get_by_id(id, db).await {
                            Ok(Some(media)) => {
                                if let Err(e) = cropper.add_media(&media).await {
                                    error!("Failed to add media to cropper: {}", e);
                                }
                            }

                            Ok(None) => {
                                error!("Asked python to crop non existing media");
                            }

                            Err(e) => {
                                error!("An error occured receiving media from main thread: {}", e);
                            }
                        }
                    }

                    Some(None) => {
                        // We need to end, send end to python
                        if let Err(e) = cropper.send_request(Request::End).await {
                            error!("Failed to ask cropper for termination: {}", e);
                        }
                    }

                    _ => (),
                }
            }
        })
    }
}
