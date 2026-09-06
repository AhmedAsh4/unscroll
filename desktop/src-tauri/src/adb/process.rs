use super::{Adb, AdbCommand, BridgeOperation, DeviceOperation};
use std::{
    collections::VecDeque,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;

const MAX_CAPTURE_BYTES: usize = 65_536;
const MAX_ICON_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbOutput {
    stdout_bytes: Vec<u8>,
    stdout: String,
    stderr: String,
    exit: Option<i32>,
    timed_out: bool,
    overflow: bool,
}
impl AdbOutput {
    fn bounded(value: String) -> (String, bool) {
        if value.len() <= MAX_CAPTURE_BYTES {
            return (value, false);
        }
        let mut end = MAX_CAPTURE_BYTES;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        (value[..end].into(), true)
    }
    fn with_facts(
        stdout: String,
        stderr: String,
        exit: Option<i32>,
        timed_out: bool,
        overflow: bool,
    ) -> Self {
        let (stdout, stdout_overflow) = Self::bounded(stdout);
        let (stderr, stderr_overflow) = Self::bounded(stderr);
        Self {
            stdout_bytes: stdout.as_bytes().to_vec(),
            stdout,
            stderr,
            exit,
            timed_out,
            overflow: overflow || stdout_overflow || stderr_overflow,
        }
    }
    fn from_process(
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        exit: Option<i32>,
        timed_out: bool,
        overflow: bool,
        binary: bool,
    ) -> Self {
        let stdout_text = if binary {
            String::new()
        } else {
            String::from_utf8_lossy(&stdout).into_owned()
        };
        let mut result = Self::with_facts(
            stdout_text,
            String::from_utf8_lossy(&stderr).into_owned(),
            exit,
            timed_out,
            overflow,
        );
        result.stdout_bytes = stdout;
        result
    }
    pub fn success(stdout: impl Into<String>) -> Self {
        Self::with_facts(stdout.into(), String::new(), Some(0), false, false)
    }
    pub fn failure(exit: i32, stderr: impl Into<String>) -> Self {
        Self::failure_with_stdout(exit, "", stderr)
    }
    pub fn failure_with_stdout(
        exit: i32,
        stdout: impl Into<String>,
        stderr: impl Into<String>,
    ) -> Self {
        Self::with_facts(stdout.into(), stderr.into(), Some(exit), false, false)
    }
    pub fn timeout() -> Self {
        Self::with_facts(String::new(), String::new(), None, true, false)
    }
    pub fn binary_success(stdout: Vec<u8>) -> Self {
        let overflow = stdout.len() > MAX_ICON_BYTES;
        let stdout_bytes = stdout.into_iter().take(MAX_ICON_BYTES).collect();
        Self {
            stdout_bytes,
            stdout: String::new(),
            stderr: String::new(),
            exit: Some(0),
            timed_out: false,
            overflow,
        }
    }
    pub fn overflow() -> Self {
        Self::with_facts(String::new(), String::new(), None, false, true)
    }
    pub fn stdout(&self) -> &str {
        &self.stdout
    }
    pub fn stdout_bytes(&self) -> &[u8] {
        &self.stdout_bytes
    }
    pub fn stderr(&self) -> &str {
        &self.stderr
    }
    pub fn exit(&self) -> Option<i32> {
        self.exit
    }
    pub fn timed_out(&self) -> bool {
        self.timed_out
    }
    pub fn is_overflow(&self) -> bool {
        self.overflow
    }
    pub fn succeeded(&self) -> bool {
        self.exit == Some(0) && !self.timed_out && !self.overflow
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdbError {
    NonZero(AdbOutput),
    Timeout(AdbOutput),
    OutputOverflow(AdbOutput),
    InconsistentState,
    LaunchFailed,
    UnexpectedOutput,
    NetworkTransport,
}
pub fn require_success(output: AdbOutput) -> Result<AdbOutput, AdbError> {
    if output.overflow {
        Err(AdbError::OutputOverflow(output))
    } else if output.timed_out {
        Err(AdbError::Timeout(output))
    } else if output.exit != Some(0) {
        Err(AdbError::NonZero(output))
    } else {
        Ok(output)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub command_class: &'static str,
    pub serial: Option<String>,
    pub exit: Option<i32>,
    pub timed_out: bool,
    pub stderr: String,
}
pub fn redacted_diagnostic(command: &AdbCommand, output: &AdbOutput) -> Diagnostic {
    Diagnostic {
        command_class: command.class(),
        serial: command.serial().map(|_| "[redacted]".into()),
        exit: output.exit,
        timed_out: output.timed_out,
        stderr: if output.stderr.is_empty() {
            String::new()
        } else {
            "[redacted]".into()
        },
    }
}

pub struct BundledAdb {
    executable: PathBuf,
}
impl BundledAdb {
    pub fn for_app<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<Self, AdbError> {
        Self::from_resource_dir(
            app.path()
                .resource_dir()
                .map_err(|_| AdbError::LaunchFailed)?,
        )
    }
    pub fn for_development() -> Result<Self, AdbError> {
        Self::from_resource_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"))
    }
    fn from_resource_dir(resource_dir: PathBuf) -> Result<Self, AdbError> {
        let executable = resource_dir.join("adb").join("adb.exe");
        executable
            .is_file()
            .then_some(Self { executable })
            .ok_or(AdbError::LaunchFailed)
    }
}
fn read_bounded<R: Read + Send + 'static>(
    mut stream: R,
    limit: usize,
) -> mpsc::Receiver<(Vec<u8>, bool)> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut overflow = false;
        let mut buffer = [0; 8192];
        while let Ok(count) = stream.read(&mut buffer) {
            if count == 0 {
                break;
            }
            let keep = limit.saturating_sub(bytes.len()).min(count);
            bytes.extend_from_slice(&buffer[..keep]);
            overflow |= count > keep;
        }
        let _ = sender.send((bytes, overflow));
    });
    receiver
}

#[cfg(test)]
mod tests {
    use super::{read_bounded, AdbOutput, MAX_CAPTURE_BYTES};
    use std::io::Cursor;

    #[test]
    fn bounded_reader_retains_at_most_the_capture_limit_when_output_overflows() {
        let (bytes, overflow) = read_bounded(
            Cursor::new(vec![b'x'; MAX_CAPTURE_BYTES + 1]),
            MAX_CAPTURE_BYTES,
        )
        .recv()
        .expect("reader result");

        assert_eq!(bytes.len(), MAX_CAPTURE_BYTES);
        assert!(overflow);
    }

    #[test]
    fn process_output_caps_lossy_utf8_expansion() {
        let output = AdbOutput::from_process(
            vec![0xff; MAX_CAPTURE_BYTES],
            Vec::new(),
            Some(0),
            false,
            false,
            false,
        );

        assert!(output.overflow);
        assert!(output.stdout().len() <= MAX_CAPTURE_BYTES);
    }
}

impl Adb for BundledAdb {
    fn execute(&mut self, command: AdbCommand) -> Result<AdbOutput, AdbError> {
        let mut child = Command::new(&self.executable)
            .args(command.arguments())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| AdbError::LaunchFailed)?;
        let binary = matches!(
            command,
            AdbCommand::Device {
                operation: DeviceOperation::Bridge(BridgeOperation::ReadIcon(_)),
                ..
            }
        );
        let stdout = read_bounded(
            child.stdout.take().ok_or(AdbError::LaunchFailed)?,
            if binary {
                MAX_ICON_BYTES
            } else {
                MAX_CAPTURE_BYTES
            },
        );
        let stderr = read_bounded(
            child.stderr.take().ok_or(AdbError::LaunchFailed)?,
            MAX_CAPTURE_BYTES,
        );
        let started = Instant::now();
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|_| AdbError::LaunchFailed)? {
                break status;
            }
            if started.elapsed() >= command.timeout() {
                timed_out = true;
                let _ = child.kill();
                break child.wait().map_err(|_| AdbError::LaunchFailed)?;
            }
            thread::sleep(Duration::from_millis(10));
        };
        let (stdout, stdout_overflow) = stdout.recv().map_err(|_| AdbError::LaunchFailed)?;
        let (stderr, stderr_overflow) = stderr.recv().map_err(|_| AdbError::LaunchFailed)?;
        Ok(AdbOutput::from_process(
            stdout,
            stderr,
            (!timed_out).then(|| status.code().unwrap_or(-1)),
            timed_out,
            stdout_overflow || stderr_overflow,
            binary,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdbResponse {
    Output(AdbOutput),
}
impl AdbResponse {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self::Output(AdbOutput::success(stdout))
    }
    pub fn failure(exit: i32, stdout: impl Into<String>, stderr: impl Into<String>) -> Self {
        Self::Output(AdbOutput::failure_with_stdout(exit, stdout, stderr))
    }
    pub fn timeout() -> Self {
        Self::Output(AdbOutput::timeout())
    }
    pub fn overflow() -> Self {
        Self::Output(AdbOutput::overflow())
    }
    pub fn binary_success(stdout: Vec<u8>) -> Self {
        Self::Output(AdbOutput::binary_success(stdout))
    }
    pub fn ordinary_failure() -> Self {
        Self::Output(AdbOutput::failure(1, "ordinary failure"))
    }
    pub fn required_failure() -> Self {
        Self::Output(AdbOutput::failure(2, "required failure"))
    }
}
pub struct FakeAdb {
    scripted: VecDeque<AdbResponse>,
    seen: Vec<AdbCommand>,
}
impl FakeAdb {
    pub fn scripted(responses: impl IntoIterator<Item = AdbResponse>) -> Self {
        Self {
            scripted: responses.into_iter().collect(),
            seen: Vec::new(),
        }
    }
    pub fn seen(&self) -> &[AdbCommand] {
        &self.seen
    }
    pub fn execute(&mut self, command: AdbCommand) -> Result<AdbOutput, AdbError> {
        <Self as Adb>::execute(self, command)
    }
}
impl Adb for FakeAdb {
    fn execute(&mut self, command: AdbCommand) -> Result<AdbOutput, AdbError> {
        self.seen.push(command);
        match self.scripted.pop_front() {
            Some(AdbResponse::Output(output)) => Ok(output),
            None => Err(AdbError::InconsistentState),
        }
    }
}
