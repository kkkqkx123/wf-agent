pub mod commands;
pub mod output;

use std::io::Read;
use std::sync::Arc;

use crate::api::service::{ApiService, ServiceConfig};
use crate::api::types::*;
use crate::error::exit_codes;

use commands::*;
use output::*;

/// CLI entry point — parse args, call ApiService, format output, return exit code
pub fn run() -> i32 {
    run_with_cli(commands::parse_args())
}

/// Run CLI with a pre-built Cli struct (useful for testing)
pub fn run_with_cli(cli: Cli) -> i32 {
    let format = if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::Plain
    };

    let result = match &cli.command {
        Commands::Init { git_ref } => {
            let service = match ApiService::open(ServiceConfig {
                db_path: cli.db_path.clone(),
            }) {
                Ok(s) => Arc::new(s),
                Err(e) => {
                    eprintln!("error: {}", e);
                    return exit_codes::GENERAL_ERROR;
                }
            };
            service
                .init(InitRequest {
                    db_path: Some(cli.db_path.clone()),
                    git_repo: cli.git_repo.clone(),
                    git_ref: git_ref.clone(),
                })
                .map(|_| ())
        }
        Commands::Status => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            let resp = service.status();
            match &resp {
                Ok(r) => println!("{}", format_status_response(r, format)),
                Err(e) => eprintln!("error reading status: {}", e),
            }
            resp.map(|_| ())
        }
        Commands::Edit { file, content } => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            let content = if let Some(c) = content {
                Some(c.clone())
            } else {
                let mut buf = String::new();
                match std::io::stdin().read_to_string(&mut buf) {
                    Ok(_) if !buf.is_empty() => Some(buf),
                    _ => None,
                }
            };
            let resp = service.edit(EditRequest {
                file: file.clone(),
                content,
            });
            if let Ok(ref r) = resp {
                println!("Edited '{}' -> new snapshot {}", file, &r.snapshot_id[..12]);
                if let Some(ref staged) = r.staged_snapshot_id {
                    println!("Merged to staged -> snapshot {}", &staged[..12]);
                }
            }
            resp.map(|_| ())
        }
        Commands::Agent { agent_id, action } => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            match action {
                AgentCommands::Edit { file, content } => {
                    let content = if let Some(c) = content {
                        Some(c.clone())
                    } else {
                        let mut buf = String::new();
                        match std::io::stdin().read_to_string(&mut buf) {
                            Ok(_) if !buf.is_empty() => Some(buf),
                            _ => None,
                        }
                    };
                    let resp = service.agent_edit(AgentEditRequest {
                        agent_id: agent_id.clone(),
                        file: file.clone(),
                        content,
                    });
                    if let Ok(ref r) = resp {
                        println!(
                            "Agent '{}' edited '{}' -> snapshot {}",
                            agent_id,
                            file,
                            &r.snapshot_id[..12]
                        );
                    }
                    resp.map(|_| ())
                }
                AgentCommands::Submit => {
                    let resp = service.agent_submit(AgentSubmitRequest {
                        agent_id: agent_id.clone(),
                    });
                    if let Ok(ref r) = resp {
                        println!(
                            "Agent '{}' submitted for approval -> snapshot {}",
                            agent_id,
                            &r.snapshot_id[..12]
                        );
                    }
                    resp.map(|_| ())
                }
            }
        }
        Commands::Commit { message, author } => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            let resp = service.commit(CommitRequest {
                message: message.clone(),
                author: Some(author.clone()),
            });
            if let Ok(ref r) = resp {
                println!(
                    "Committed checkpoint {}: {}",
                    &r.checkpoint_id[..12],
                    r.message
                );
            }
            resp.map(|_| ())
        }
        Commands::Log { count } => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            let resp = service.log(LogRequest {
                count: Some(*count),
            });
            if let Ok(ref r) = resp {
                println!("{}", format_log_response(r, format));
            }
            resp.map(|_| ())
        }
        Commands::Branch { action } => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            match action {
                BranchCommands::List => {
                    let resp = service.branch_list();
                    if let Ok(ref r) = resp {
                        println!("{}", format_branches_response(r, format));
                    }
                    resp.map(|_| ())
                }
                BranchCommands::Create { name } => {
                    let resp = service.branch_create(BranchCreateRequest { name: name.clone() });
                    if let Ok(ref r) = resp {
                        println!("Created branch '{}' at {}", r.name, &r.head[..12]);
                    }
                    resp.map(|_| ())
                }
                BranchCommands::Switch { name } => {
                    let resp = service.branch_switch(BranchSwitchRequest { name: name.clone() });
                    if let Ok(ref r) = resp {
                        println!(
                            "Switched to branch '{}' at checkpoint {}",
                            r.name,
                            &r.checkpoint_id[..12]
                        );
                    }
                    resp.map(|_| ())
                }
            }
        }
        Commands::Merge { branch, message } => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            let resp = service.merge(MergeRequest {
                branch: branch.clone(),
                message: Some(message.clone()),
            });
            if let Ok(ref r) = resp {
                println!(
                    "Merged '{}' into '{}' -> checkpoint {}",
                    r.source_branch,
                    r.target_branch,
                    &r.checkpoint_id[..12]
                );
            }
            resp.map(|_| ())
        }
        Commands::Backup { snapshot_id, label } => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            let resp = service.backup(BackupRequest {
                snapshot_id: snapshot_id.clone(),
                label: label.clone(),
            });
            if let Ok(ref r) = resp {
                if let Some(ref lbl) = r.label {
                    println!(
                        "Backup {} created for snapshot {} (label: {})",
                        &r.backup_id[..12],
                        &r.source_snapshot_id[..12],
                        lbl
                    );
                } else {
                    println!(
                        "Backup {} created for snapshot {}",
                        &r.backup_id[..12],
                        &r.source_snapshot_id[..12]
                    );
                }
            }
            resp.map(|_| ())
        }
        Commands::Restore { backup_id } => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            let resp = service.restore(RestoreRequest {
                backup_id: backup_id.clone(),
            });
            if let Ok(ref r) = resp {
                println!("Restored backup {} -> file: {}", &r.backup_id[..12], r.file);
            }
            resp.map(|_| ())
        }
        Commands::Compact { vacuum_full } => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            print_progress("Compacting database");
            let resp = service.compact(CompactRequest {
                vacuum_full: if *vacuum_full { Some(true) } else { None },
            });
            if let Ok(ref r) = resp {
                print_done();
                println!("{}", r.message);
                if r.vacuum_performed {
                    println!(
                        "  Free pages: {} -> {} (total: {})",
                        r.freelist_before, r.freelist_after, r.total_pages
                    );
                }
            }
            resp.map(|_| ())
        }
        Commands::Gc => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            print_progress("Running garbage collection");
            let resp = service.gc(GcRequest {});
            if let Ok(ref r) = resp {
                print_done();
                println!(
                    "GC complete: {} checkpoints removed, {} snapshots freed, {} bytes",
                    r.removed_checkpoints, r.removed_snapshots, r.freed_bytes
                );
                if r.delta_chain_depth_triggered {
                    println!("  Note: delta chain depth exceeded threshold");
                }
            }
            resp.map(|_| ())
        }
        Commands::GitCommit { message } => {
            let git_repo = match &cli.git_repo {
                Some(p) => p.clone(),
                None => {
                    eprintln!("error: --git-repo path is required for git-commit");
                    return exit_codes::USAGE_ERROR;
                }
            };
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            print_progress("Committing to local Git branch");
            let resp = service.git_commit(GitCommitRequest {
                git_repo,
                message: Some(message.clone()),
            });
            if let Ok(ref r) = resp {
                print_done();
                println!(
                    "Committed to local Git branch (commit: {})",
                    r.git_commit_hash
                );
                println!(
                    "  Run `git push` manually to push to remote."
                );
            }
            resp.map(|_| ())
        }
        Commands::Clean { branch, layer, all } => {
            if !*all && branch.is_none() && layer.is_none() {
                eprintln!("error: specify --branch, --layer, or --all for clean");
                return exit_codes::USAGE_ERROR;
            }
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            print_progress("Cleaning layertwine storage");
            let resp = service.clean(CleanRequest {
                branch: branch.clone(),
                layer: layer.clone(),
                all: *all,
            });
            if let Ok(ref r) = resp {
                print_done();
                println!("{}", r.message);
            }
            resp.map(|_| ())
        }
        Commands::Show {
            show_what,
            target_id,
        } => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            let resp = service.show(ShowRequest {
                show_what: show_what.clone(),
                target_id: target_id.clone(),
            });
            if let Ok(ref r) = resp {
                println!("{}", format_show_response(r, format));
            }
            resp.map(|_| ())
        }
        Commands::Pull { remote, git_ref } => {
            let git_repo = match &cli.git_repo {
                Some(p) => p.clone(),
                None => {
                    eprintln!("error: --git-repo path is required for pull");
                    return exit_codes::USAGE_ERROR;
                }
            };
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            print_progress("Pulling from Git remote");
            let resp = service.pull(PullRequest {
                remote: Some(remote.clone()),
                git_repo,
                git_ref: Some(git_ref.clone()),
            });
            if let Ok(ref r) = resp {
                print_done();
                println!("Pulled from remote '{}' ref '{}'", r.remote, r.git_ref);
            }
            resp.map(|_| ())
        }
        Commands::Checkpoint { action } => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            match action {
                CheckpointCommands::Restore {
                    checkpoint_id,
                    source_filter,
                } => {
                    let resp = service.checkpoint_restore(CheckpointRestoreRequest {
                        checkpoint_id: checkpoint_id.clone(),
                        source_filter: source_filter.clone(),
                    });
                    if let Ok(ref r) = resp {
                        println!(
                            "Restored checkpoint {}{}",
                            &r.checkpoint.id[..12],
                            r.checkpoint
                                .git_anchor
                                .as_ref()
                                .map(|g| format!(" (git: {})", g))
                                .unwrap_or_default()
                        );
                        println!("  Message: {}", r.checkpoint.message);
                        println!("  Snapshots: {}", r.snapshots.len());
                        for snap in &r.snapshots {
                            println!(
                                "    {} [{}] (source: {})",
                                &snap.snapshot_id[..12],
                                snap.content_type,
                                snap.source
                            );
                        }
                    }
                    resp.map(|_| ())
                }
                CheckpointCommands::RestoreByTime {
                    target_time,
                    source_filter,
                } => {
                    let resp = service.checkpoint_restore_by_time(CheckpointRestoreByTimeRequest {
                        target_time: *target_time,
                        source_filter: source_filter.clone(),
                    });
                    if let Ok(ref r) = resp {
                        println!(
                            "Restored checkpoint {} (nearest to {})",
                            &r.checkpoint.id[..12],
                            *target_time
                        );
                        println!("  Message: {}", r.checkpoint.message);
                        println!("  Snapshots: {}", r.snapshots.len());
                    }
                    resp.map(|_| ())
                }
                CheckpointCommands::Diff { from_id, to_id } => {
                    let resp = service.checkpoint_diff(CheckpointDiffRequest {
                        from_id: from_id.clone(),
                        to_id: to_id.clone(),
                    });
                    if let Ok(ref r) = resp {
                        println!(
                            "Diff {} -> {} ({} changes)",
                            &r.from_id[..12],
                            &r.to_id[..12],
                            r.total_changes
                        );
                        if !r.added.is_empty() {
                            println!("  Added ({}):", r.added.len());
                            for id in &r.added {
                                println!("    + {}", &id[..12]);
                            }
                        }
                        if !r.modified.is_empty() {
                            println!("  Modified ({}):", r.modified.len());
                            for id in &r.modified {
                                println!("    ~ {}", &id[..12]);
                            }
                        }
                        if !r.removed.is_empty() {
                            println!("  Removed ({}):", r.removed.len());
                            for id in &r.removed {
                                println!("    - {}", &id[..12]);
                            }
                        }
                    }
                    resp.map(|_| ())
                }
                CheckpointCommands::Rollback { checkpoint_id } => {
                    let resp = service.checkpoint_rollback(CheckpointRollbackRequest {
                        checkpoint_id: checkpoint_id.clone(),
                    });
                    if let Ok(ref r) = resp {
                        println!(
                            "Rolled back staged to checkpoint {}",
                            &r.checkpoint_id[..12]
                        );
                        println!(
                            "  Staged snapshots reset to: {}",
                            r.snapshot_ids.first().map(|s| &s[..12]).unwrap_or("(none)")
                        );
                    }
                    resp.map(|_| ())
                }
            }
        }
        Commands::Approval { action } => {
            let service = match open_service(&cli) {
                Ok(s) => s,
                Err(code) => return code,
            };
            match action {
                ApprovalCommands::List => {
                    let resp = service.list_pending_approvals();
                    if let Ok(ref r) = resp {
                        if r.approvals.is_empty() {
                            println!("No pending approvals.");
                        } else {
                            println!("Pending approvals ({}):", r.total);
                            println!("{:-<72}", "");
                            println!(
                                "{:<24} {:<32} {:<20} History",
                                "Agent ID", "Partition", "Current Snapshot"
                            );
                            println!("{:-<72}", "");
                            for a in &r.approvals {
                                println!(
                                    "{:<24} {:<32} {:<20} {} snapshots",
                                    a.agent_id,
                                    a.partition_name,
                                    &a.current_snapshot[..12],
                                    a.history_len
                                );
                            }
                        }
                    }
                    resp.map(|_| ())
                }
                ApprovalCommands::Approve {
                    agent_id,
                    integrated_name,
                } => {
                    let resp = service.approve_agent(ApproveAgentRequest {
                        agent_id: agent_id.clone(),
                        integrated_name: integrated_name.clone(),
                    });
                    if let Ok(ref r) = resp {
                        println!(
                            "Approved agent '{}' -> integrated snapshot {}",
                            r.agent_id,
                            &r.integrated_snapshot_id[..12]
                        );
                    }
                    resp.map(|_| ())
                }
                ApprovalCommands::Reject { agent_id } => {
                    let resp = service.reject_agent(RejectAgentRequest {
                        agent_id: agent_id.clone(),
                    });
                    if let Ok(ref r) = resp {
                        println!(
                            "Rejected agent '{}' -> rolled back to {}",
                            r.agent_id,
                            &r.baseline_snapshot_id[..12]
                        );
                    }
                    resp.map(|_| ())
                }
                ApprovalCommands::MergeToUnified { names } => {
                    let resp = service.merge_to_unified(MergeToUnifiedRequest {
                        integration_names: names.clone(),
                    });
                    if let Ok(ref r) = resp {
                        println!(
                            "Merged {} integration(s) to unified -> snapshot {}",
                            r.merged_count,
                            &r.unified_snapshot_id[..12]
                        );
                    }
                    resp.map(|_| ())
                }
                ApprovalCommands::MergeToStaged => {
                    let resp = service.merge_to_staged(MergeToStagedRequest {});
                    if let Ok(ref r) = resp {
                        println!(
                            "Merged unified to staged -> snapshot {}",
                            &r.staged_snapshot_id[..12]
                        );
                    }
                    resp.map(|_| ())
                }
            }
        }
    };

    match result {
        Ok(()) => exit_codes::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            exit_codes::GENERAL_ERROR
        }
    }
}

fn open_service(cli: &Cli) -> std::result::Result<Arc<ApiService>, i32> {
    match ApiService::open(ServiceConfig {
        db_path: cli.db_path.clone(),
    }) {
        Ok(s) => Ok(Arc::new(s)),
        Err(e) => {
            eprintln!("error: {}", e);
            Err(exit_codes::GENERAL_ERROR)
        }
    }
}
