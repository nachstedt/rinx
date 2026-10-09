//! The workspace scan's thread, and the events it sends the protocol loop.
//!
//! Scanning a large workspace takes long enough that an editor opening a
//! document meanwhile must not wait for it, so it runs beside the loop. It
//! only reads the disk and builds plain data; everything it found reaches the
//! server's state as a [`ScanEvent`], on the loop's own thread, which is where
//! the open buffers and the workspace index live.

use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;

use crate::project::{DiscoveredProject, ScannedProject, discover_projects, scan_projects};

/// What the scan tells the protocol loop.
#[derive(Debug)]
pub enum ScanEvent {
    /// Folder `folder`'s projects are found, before any document is parsed,
    /// so a document opened meanwhile is parsed as its project says.
    Projects {
        folder: usize,
        projects: Vec<DiscoveredProject>,
    },
    /// `done` of the `total` documents of folder `folder` are scanned.
    Progress {
        folder: usize,
        done: usize,
        total: usize,
    },
    /// Folder `folder` is scanned, `elapsed` after the scan began. It names
    /// the projects again, so it stands on its own.
    Finished {
        folder: usize,
        projects: Vec<ScannedProject>,
        elapsed: Duration,
    },
}

/// How many progress reports a folder's scan sends at most, so a large
/// workspace does not flood the client.
const REPORTS_PER_FOLDER: usize = 20;

/// Scans the folders at `roots`, in order, on a thread of its own, sending
/// what it finds to `events`. The thread ends when the scan does, or as soon
/// as nobody is listening.
pub fn spawn_scan(roots: Vec<PathBuf>, events: Sender<ScanEvent>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let started = Instant::now();
        for (folder, root) in roots.iter().enumerate() {
            let projects = discover_projects(root);
            let found = ScanEvent::Projects {
                folder,
                projects: projects.clone(),
            };
            if events.send(found).is_err() {
                return;
            }
            let projects = scan_projects(projects, &|done, total| {
                if is_report_due(done, total) {
                    let _ = events.send(ScanEvent::Progress {
                        folder,
                        done,
                        total,
                    });
                }
            });
            let finished = ScanEvent::Finished {
                folder,
                projects,
                elapsed: started.elapsed(),
            };
            if events.send(finished).is_err() {
                return;
            }
        }
    })
}

/// Whether finishing the `done`th of `total` documents crosses one of the
/// [`REPORTS_PER_FOLDER`] steps.
fn is_report_due(done: usize, total: usize) -> bool {
    total > 0 && done * REPORTS_PER_FOLDER / total != (done - 1) * REPORTS_PER_FOLDER / total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_report_due_reports_at_most_once_per_step_and_at_the_end() {
        // Given
        let total = 1000;

        // When
        let due: Vec<usize> = (1..=total)
            .filter(|&done| is_report_due(done, total))
            .collect();

        // Then
        assert_eq!(due.len(), REPORTS_PER_FOLDER);
        assert_eq!(due.last(), Some(&total));
    }

    #[test]
    fn test_is_report_due_reports_every_document_of_a_small_folder() {
        // When / Then
        assert!((1..=3).all(|done| is_report_due(done, 3)));
    }

    #[test]
    fn test_spawn_scan_reports_each_folders_projects_then_documents_in_order() {
        // Given
        let first = std::env::temp_dir().join("rinx_lsp_spawn_scan_first");
        let second = std::env::temp_dir().join("rinx_lsp_spawn_scan_second");
        for (root, name) in [(&first, "a.rst"), (&second, "b.rst")] {
            let _ = std::fs::remove_dir_all(root);
            std::fs::create_dir_all(root).expect("mkdir");
            std::fs::write(root.join(name), "Text\n").expect("write");
        }
        let (sender, receiver) = crossbeam_channel::unbounded();

        // When
        spawn_scan(vec![first, second], sender)
            .join()
            .expect("scan thread");

        // Then — each folder's projects first, then its documents
        let events: Vec<(usize, &str, Vec<String>)> = receiver
            .try_iter()
            .filter_map(|event| match event {
                ScanEvent::Projects { folder, projects } => Some((
                    folder,
                    "projects",
                    projects
                        .into_iter()
                        .flat_map(|project| project.sources)
                        .map(|(name, _)| name)
                        .collect(),
                )),
                ScanEvent::Finished {
                    folder, projects, ..
                } => Some((
                    folder,
                    "finished",
                    projects
                        .into_iter()
                        .flat_map(|project| project.documents)
                        .map(|(name, _)| name)
                        .collect(),
                )),
                ScanEvent::Progress { .. } => None,
            })
            .collect();
        let names = |name: &str| vec![name.to_string()];
        assert_eq!(
            events,
            [
                (0, "projects", names("a.rst")),
                (0, "finished", names("a.rst")),
                (1, "projects", names("b.rst")),
                (1, "finished", names("b.rst"))
            ]
        );
    }
}
