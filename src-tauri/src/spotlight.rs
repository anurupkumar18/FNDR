//! Bounded Spotlight (`mdfind`) lookups shared by Screen Guide and reopen.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncReadExt;

pub fn parse_mdfind_nul_paths(output: &[u8]) -> impl Iterator<Item = PathBuf> + '_ {
    output
        .split(|byte| *byte == 0)
        .filter(|encoded| !encoded.is_empty())
        .filter_map(|encoded| std::str::from_utf8(encoded).ok())
        .map(PathBuf::from)
}

async fn read_bounded_mdfind_output(
    mut stdout: tokio::process::ChildStdout,
    byte_budget: usize,
) -> Result<(Vec<u8>, bool), String> {
    let mut stored = Vec::with_capacity(byte_budget.min(8 * 1024));
    let mut chunk = [0_u8; 8 * 1024];
    loop {
        let read = stdout.read(&mut chunk).await.map_err(|_| {
            "FNDR's local file search could not read Spotlight results.".to_string()
        })?;
        if read == 0 {
            break;
        }
        let remaining = byte_budget.saturating_sub(stored.len());
        stored.extend_from_slice(&chunk[..read.min(remaining)]);
        if read > remaining {
            return Ok((stored, true));
        }
    }
    Ok((stored, false))
}

/// Run `/usr/bin/mdfind -0 -name <name>`, optionally `-onlyin <root>`.
pub async fn run_mdfind_name(
    name: &str,
    only_in: Option<&Path>,
    byte_budget: usize,
) -> Result<Vec<u8>, String> {
    let mut command = tokio::process::Command::new("/usr/bin/mdfind");
    command.arg("-0");
    if let Some(root) = only_in {
        command.arg("-onlyin").arg(root);
    }
    command
        .arg("-name")
        .arg(name)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|_| "FNDR's local file search is unavailable on this Mac.".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "FNDR's local file search could not start.".to_string())?;
    let mut output_future = Box::pin(read_bounded_mdfind_output(stdout, byte_budget));
    enum FirstCompletion {
        Output(Result<(Vec<u8>, bool), String>),
        Process(std::io::Result<std::process::ExitStatus>),
    }
    let first = {
        let mut wait_future = Box::pin(child.wait());
        tokio::select! {
            output = &mut output_future => FirstCompletion::Output(output),
            status = &mut wait_future => FirstCompletion::Process(status),
        }
    };
    let (output, status, limit_reached) = match first {
        FirstCompletion::Output(output) => {
            let (output, limit_reached) = output?;
            if limit_reached {
                let _ = child.start_kill();
            }
            (output, child.wait().await, limit_reached)
        }
        FirstCompletion::Process(status) => {
            let (output, limit_reached) = output_future.await?;
            (output, status, limit_reached)
        }
    };
    let status =
        status.map_err(|_| "FNDR's local file search stopped unexpectedly.".to_string())?;
    if !status.success() && !limit_reached {
        return Err("FNDR's local file search could not query Spotlight.".to_string());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mdfind_nul_paths_skips_empty_and_non_utf8() {
        let mut bytes = b"/Users/qa/a.pdf\0/Users/qa/b.pdf\0".to_vec();
        bytes.extend_from_slice(&[0xff, 0xfe, 0x00]);
        let paths: Vec<PathBuf> = parse_mdfind_nul_paths(&bytes).collect();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/Users/qa/a.pdf"),
                PathBuf::from("/Users/qa/b.pdf")
            ]
        );
    }
}
