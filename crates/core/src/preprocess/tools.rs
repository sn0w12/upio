//! Detection of external tools used by preprocessing strategies.

/// An external tool that a preprocessing strategy may require.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Ffmpeg,
    Ffprobe,
}

impl Tool {
    /// The binary name on `PATH`.
    pub fn binary(self) -> &'static str {
        match self {
            Tool::Ffmpeg => "ffmpeg",
            Tool::Ffprobe => "ffprobe",
        }
    }

    /// Check whether the tool's binary is available and runs.
    ///
    /// On Windows the child is spawned without a console window so probing
    /// from a GUI does not flash a terminal.
    pub fn available(self) -> bool {
        let mut command = std::process::Command::new(self.binary());
        command.arg("-version");

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;

            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        match command.output() {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }
}

/// Report the availability of every known tool.
pub fn available_tools() -> Vec<(Tool, bool)> {
    vec![
        (Tool::Ffmpeg, Tool::Ffmpeg.available()),
        (Tool::Ffprobe, Tool::Ffprobe.available()),
    ]
}
