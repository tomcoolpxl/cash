use cash_parser::ast;
use std::path::Path;

use crate::{
    ExecutionParameters, Shell, ShellFd, arithmetic, env, error, escape, expansion, extensions,
    namedoptions, patterns,
    sys::{
        fs::{MetadataExt, PathExt},
        users,
    },
    variables::{self, ArrayLiteral},
};

#[expect(
    clippy::double_must_use,
    reason = "async_recursion's expansion marks the boxed future it returns #[must_use]"
)]
#[async_recursion::async_recursion]
pub(crate) async fn eval_extended_test_expr(
    expr: &ast::ExtendedTestExpr,
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
) -> Result<bool, error::Error> {
    match expr {
        ast::ExtendedTestExpr::UnaryTest(op, operand) => {
            apply_unary_predicate(op, operand, shell, params).await
        }
        ast::ExtendedTestExpr::BinaryTest(op, left, right) => {
            apply_binary_predicate(op, left, right, shell, params).await
        }
        ast::ExtendedTestExpr::And(left, right) => {
            let result = eval_extended_test_expr(left, shell, params).await?
                && eval_extended_test_expr(right, shell, params).await?;
            Ok(result)
        }
        ast::ExtendedTestExpr::Or(left, right) => {
            let result = eval_extended_test_expr(left, shell, params).await?
                || eval_extended_test_expr(right, shell, params).await?;
            Ok(result)
        }
        ast::ExtendedTestExpr::Not(expr) => {
            let result = !eval_extended_test_expr(expr, shell, params).await?;
            Ok(result)
        }
        ast::ExtendedTestExpr::Parenthesized(expr) => {
            eval_extended_test_expr(expr, shell, params).await
        }
    }
}

async fn apply_unary_predicate(
    op: &ast::UnaryPredicate,
    operand: &ast::Word,
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
) -> Result<bool, error::Error> {
    let expanded_operand = expansion::basic_expand_word(shell, params, operand).await?;

    if shell.options().print_commands_and_arguments {
        shell
            .trace_command(
                params,
                std::format!(
                    "[[ {op} {} ]]",
                    escape::quote_if_needed(&expanded_operand, escape::QuoteMode::SingleQuote)
                ),
            )
            .await;
    }

    apply_unary_predicate_to_str(op, expanded_operand.as_str(), shell, params, false).await
}

/// Applies a unary test to an already-expanded operand. `in_test_builtin` is true for
/// `test`/`[`, where `assoc_expand_once` applies to `-v`; it does not apply to `[[`.
#[expect(clippy::too_many_lines)]
pub(crate) async fn apply_unary_predicate_to_str(
    op: &ast::UnaryPredicate,
    operand: &str,
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    in_test_builtin: bool,
) -> Result<bool, error::Error> {
    if let Some(answer) = dev_test(op, operand, shell, params) {
        return Ok(answer);
    }
    match op {
        ast::UnaryPredicate::StringHasNonZeroLength => Ok(!operand.is_empty()),
        ast::UnaryPredicate::StringHasZeroLength => Ok(operand.is_empty()),
        ast::UnaryPredicate::FileExists => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.exists())
        }
        ast::UnaryPredicate::FileExistsAndIsBlockSpecialFile => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.exists_and_is_block_device())
        }
        ast::UnaryPredicate::FileExistsAndIsCharSpecialFile => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.exists_and_is_char_device())
        }
        ast::UnaryPredicate::FileExistsAndIsDir => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.is_dir())
        }
        ast::UnaryPredicate::FileExistsAndIsRegularFile => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.is_file())
        }
        ast::UnaryPredicate::FileExistsAndIsSetgid => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.exists_and_is_setgid())
        }
        ast::UnaryPredicate::FileExistsAndIsSymlink => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.is_symlink())
        }
        ast::UnaryPredicate::FileExistsAndHasStickyBit => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.exists_and_is_sticky_bit())
        }
        ast::UnaryPredicate::FileExistsAndIsFifo => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.exists_and_is_fifo())
        }
        ast::UnaryPredicate::FileExistsAndIsReadable => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.readable())
        }
        ast::UnaryPredicate::FileExistsAndIsNotZeroLength => {
            let path = shell.absolute_path(Path::new(operand));
            if let Ok(metadata) = path.metadata() {
                Ok(metadata.len() > 0)
            } else {
                Ok(false)
            }
        }
        ast::UnaryPredicate::FdIsOpenTerminal => {
            // Trim whitespace before parsing, matching bash behavior.
            if let Ok(fd) = operand.trim().parse::<ShellFd>() {
                if let Some(open_file) = params.try_fd(shell, fd) {
                    Ok(open_file.is_terminal())
                } else {
                    Ok(false)
                }
            } else {
                Ok(false)
            }
        }
        ast::UnaryPredicate::FileExistsAndIsSetuid => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.exists_and_is_setuid())
        }
        ast::UnaryPredicate::FileExistsAndIsWritable => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.writable())
        }
        ast::UnaryPredicate::FileExistsAndIsExecutable => {
            // cash (ROADMAP item 12): `[ -x "$(which ls)" ]` asks whether the path `which`
            // printed can be run, and for a command cash carries it can, though no file
            // is there.
            if let Some(tool) = cash_win32::path::virtual_tool(operand) {
                return Ok(shell.builtins().get(&tool).is_some_and(|r| !r.disabled));
            }
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.executable(&shell.pathext()))
        }
        ast::UnaryPredicate::FileExistsAndOwnedByEffectiveGroupId => {
            let path = shell.absolute_path(Path::new(operand));
            if !path.exists() {
                return Ok(false);
            }

            let md = path.metadata()?;
            Ok(md.gid() == users::get_effective_gid()?)
        }
        ast::UnaryPredicate::FileExistsAndModifiedSinceLastRead => {
            error::unimp("unary extended test predicate: FileExistsAndModifiedSinceLastRead")
        }
        ast::UnaryPredicate::FileExistsAndOwnedByEffectiveUserId => {
            let path = shell.absolute_path(Path::new(operand));
            if !path.exists() {
                return Ok(false);
            }

            let md = path.metadata()?;
            Ok(md.uid() == users::get_effective_uid()?)
        }
        ast::UnaryPredicate::FileExistsAndIsSocket => {
            let path = shell.absolute_path(Path::new(operand));
            Ok(path.exists_and_is_socket())
        }
        ast::UnaryPredicate::ShellOptionEnabled => {
            let shopt_name = operand;
            if let Some(option) =
                namedoptions::options(namedoptions::ShellOptionKind::SetO).get(shopt_name)
            {
                Ok(option.get(shell.options()))
            } else {
                Ok(false)
            }
        }
        ast::UnaryPredicate::ShellVariableIsSetAndAssigned => {
            let expand_once = in_test_builtin && shell.options().assoc_expand_once;
            shell_parameter_is_set(shell, params, operand, expand_once).await
        }
        // cash: `-R` asks whether the *name* is a reference, so it has to see the variable
        // itself rather than what it points at — every other lookup now follows the
        // reference through.
        ast::UnaryPredicate::ShellVariableIsSetAndNameRef => match shell.env().get_raw(operand) {
            Some((_, reffed)) => Ok(reffed.value().is_set() && reffed.is_treated_as_nameref()),
            None => Ok(false),
        },
    }
}

/// What a `/dev` name is to a file test (D7).
enum DevNode {
    /// `/dev/null` or `/dev/tty`: a character device.
    Device,
    /// A descriptor's name, `/dev/stdin` or `/dev/fd/N`: a link to what is open under the
    /// number, if anything is.
    Descriptor(Option<crate::openfiles::FileKind>),
}

/// The `/dev` name `operand` is, as a redirection takes it, if it is one.
fn dev_node(
    operand: &str,
    shell: &Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
) -> Option<DevNode> {
    let path = shell.absolute_path(Path::new(operand));
    if crate::sys::fs::is_special_file(&path) {
        return Some(DevNode::Device);
    }
    let fd = crate::sys::fs::named_descriptor(&path)?;
    let open = params.try_fd(shell, fd).map(|file| file.kind());
    Some(DevNode::Descriptor(open))
}

/// cash (D7): what the file test `op` says of a `/dev` name, which no file on Windows
/// holds; `None` when `operand` is not one, or `op` asks no file.
///
/// Every test said false, `[ -e /dev/null ]` included, as Git Bash says true. The answers
/// are Git Bash 5.3's, measured with each kind of descriptor: a device can be read and
/// written, a descriptor's name is a link to what is open under the number, and that is
/// a pipe that can be read or written as its end is, a file as the file is, or a device.
/// Each is owned by the user, as Git Bash says.
fn dev_test(
    op: &ast::UnaryPredicate,
    operand: &str,
    shell: &Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
) -> Option<bool> {
    use crate::openfiles::FileKind;
    use ast::UnaryPredicate as P;

    let asks_a_file = matches!(
        op,
        P::FileExists
            | P::FileExistsAndIsBlockSpecialFile
            | P::FileExistsAndIsCharSpecialFile
            | P::FileExistsAndIsDir
            | P::FileExistsAndIsRegularFile
            | P::FileExistsAndIsSetgid
            | P::FileExistsAndIsSymlink
            | P::FileExistsAndHasStickyBit
            | P::FileExistsAndIsFifo
            | P::FileExistsAndIsReadable
            | P::FileExistsAndIsNotZeroLength
            | P::FileExistsAndIsSetuid
            | P::FileExistsAndIsWritable
            | P::FileExistsAndIsExecutable
            | P::FileExistsAndOwnedByEffectiveGroupId
            | P::FileExistsAndOwnedByEffectiveUserId
            | P::FileExistsAndIsSocket
    );
    if !asks_a_file {
        return None;
    }

    let (kind, link) = match dev_node(operand, shell, params)? {
        DevNode::Device => (FileKind::Device, false),
        DevNode::Descriptor(Some(kind)) => (kind, true),
        DevNode::Descriptor(None) => return Some(false),
    };
    let answer = match op {
        P::FileExists | P::FileExistsAndOwnedByEffectiveUserId => true,
        P::FileExistsAndOwnedByEffectiveGroupId => true,
        P::FileExistsAndIsSymlink => link,
        P::FileExistsAndIsCharSpecialFile => matches!(kind, FileKind::Device),
        P::FileExistsAndIsFifo => matches!(kind, FileKind::Pipe { .. }),
        P::FileExistsAndIsRegularFile => {
            matches!(&kind, FileKind::File(metadata) if metadata.is_file())
        }
        P::FileExistsAndIsDir => matches!(&kind, FileKind::File(metadata) if metadata.is_dir()),
        P::FileExistsAndIsNotZeroLength => {
            matches!(&kind, FileKind::File(metadata) if metadata.len() > 0)
        }
        P::FileExistsAndIsSocket => matches!(kind, FileKind::Socket),
        P::FileExistsAndIsReadable => match kind {
            FileKind::Pipe { reads, .. } => reads,
            FileKind::Device | FileKind::File(_) | FileKind::Socket => true,
            FileKind::Other => false,
        },
        P::FileExistsAndIsWritable => match &kind {
            FileKind::Pipe { writes, .. } => *writes,
            FileKind::Device | FileKind::Socket => true,
            FileKind::File(metadata) => !metadata.permissions().readonly(),
            FileKind::Other => false,
        },
        _ => false,
    };
    Some(answer)
}

/// Implements Bash's `test -v name[subscript]` handling. The subscript is expanded, then
/// used as an associative key or evaluated once as arithmetic (`[[ -v 'a[$i]' ]]`), unless
/// `expand_once` is set; see [`expansion::resolve_subscript`]. Associative `@` and `*`
/// remain ordinary keys.
async fn shell_parameter_is_set(
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
    operand: &str,
    expand_once: bool,
) -> Result<bool, error::Error> {
    let Ok(parameter) = cash_parser::word::parse_parameter(operand, &shell.parser_options()) else {
        return Ok(false);
    };

    match parameter {
        cash_parser::word::Parameter::Named(name) => Ok(shell.env().is_set(name)),
        cash_parser::word::Parameter::NamedWithIndex { name, index } => {
            let index = match expansion::resolve_subscript(
                shell,
                params,
                &name,
                &index,
                false,
                expand_once,
            )
            .await
            {
                Ok(index) => index,
                // Refused, not run: the test is false and the line goes on, as it does
                // in the builtins that refuse it.
                Err(err)
                    if matches!(
                        err.kind(),
                        error::ErrorKind::CommandSubstitutionInSubscript(_)
                    ) =>
                {
                    let _ = shell.display_error(&mut params.stderr(shell), &err);
                    return Ok(false);
                }
                Err(err) => return Err(err),
            };

            let Some((_, variable)) = shell.env().get(&name) else {
                return Ok(false);
            };
            Ok(variable.value().get_at(&index, shell)?.is_some())
        }
        cash_parser::word::Parameter::NamedWithAllIndices { name, concatenate } => {
            let Some((_, variable)) = shell.env().get(&name) else {
                return Ok(false);
            };
            if variable.value().is_associative_array() {
                let key = if concatenate { "*" } else { "@" };
                Ok(variable.value().get_at(key, shell)?.is_some())
            } else {
                Ok(!variable.value().element_values(shell).is_empty())
            }
        }
        cash_parser::word::Parameter::Positional(position) => {
            if position == 0 {
                Ok(shell.current_shell_name().is_some())
            } else {
                Ok(shell
                    .current_shell_args()
                    .get((position - 1) as usize)
                    .is_some())
            }
        }
        cash_parser::word::Parameter::Special(_) => Ok(false),
    }
}

#[expect(clippy::too_many_lines)]
async fn apply_binary_predicate(
    op: &ast::BinaryPredicate,
    left: &ast::Word,
    right: &ast::Word,
    shell: &mut Shell<impl extensions::ShellExtensions>,
    params: &ExecutionParameters,
) -> Result<bool, error::Error> {
    match op {
        ast::BinaryPredicate::StringMatchesRegex => {
            let s = expansion::basic_expand_word(shell, params, left).await?;
            let regex = expansion::basic_expand_regex(shell, params, right)
                .await?
                .set_multiline(true);

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {s} {op} {right} ]]"))
                    .await;
            }

            let (matches, captures) = match regex.matches(s.as_str()) {
                Ok(Some(captures)) => (true, captures),
                Ok(None) => (false, vec![]),
                // Bash 5.3 reports a regex that fails to compile, and `[[` yields 2; the
                // caller turns this error into that status.
                Err(e) => return Err(e),
            };

            let captures_value = variables::ShellValueLiteral::Array(ArrayLiteral(
                captures
                    .into_iter()
                    .map(|c| (None, c.unwrap_or_default()))
                    .collect(),
            ));

            shell.env_mut().update_or_add(
                "BASH_REMATCH",
                captures_value,
                |_| Ok(()),
                env::EnvironmentLookup::Anywhere,
                env::EnvironmentScope::Global,
            )?;

            Ok(matches)
        }
        ast::BinaryPredicate::StringExactlyMatchesString => {
            let left = expansion::basic_expand_word(shell, params, left).await?;
            let right = expansion::basic_expand_word(shell, params, right).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            Ok(left == right)
        }
        ast::BinaryPredicate::StringDoesNotExactlyMatchString => {
            let left = expansion::basic_expand_word(shell, params, left).await?;
            let right = expansion::basic_expand_word(shell, params, right).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            Ok(left != right)
        }
        ast::BinaryPredicate::StringContainsSubstring => {
            let s = expansion::basic_expand_word(shell, params, left).await?;
            let substring = expansion::basic_expand_word(shell, params, right).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {s} {op} {substring} ]]"))
                    .await;
            }

            Ok(s.contains(substring.as_str()))
        }
        ast::BinaryPredicate::FilesReferToSameDeviceAndInodeNumbers => {
            let left = expansion::basic_expand_word(shell, params, left).await?;
            let right = expansion::basic_expand_word(shell, params, right).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            files_refer_to_same_device_and_inode_numbers(shell, left, right)
        }
        ast::BinaryPredicate::LeftFileIsNewerOrExistsWhenRightDoesNot => {
            let left = expansion::basic_expand_word(shell, params, left).await?;
            let right = expansion::basic_expand_word(shell, params, right).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            left_file_is_newer_or_exists_when_right_does_not(shell, left, right)
        }
        ast::BinaryPredicate::LeftFileIsOlderOrDoesNotExistWhenRightDoes => {
            let left = expansion::basic_expand_word(shell, params, left).await?;
            let right = expansion::basic_expand_word(shell, params, right).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            left_file_is_older_or_does_not_exist_when_right_does(shell, left, right)
        }
        ast::BinaryPredicate::LeftSortsBeforeRight => {
            let left = expansion::basic_expand_word(shell, params, left).await?;
            let right = expansion::basic_expand_word(shell, params, right).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            // TODO(test): According to docs, should be lexicographical order of the current locale.
            Ok(left < right)
        }
        ast::BinaryPredicate::LeftSortsAfterRight => {
            let left = expansion::basic_expand_word(shell, params, left).await?;
            let right = expansion::basic_expand_word(shell, params, right).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            // TODO(test): According to docs, should be lexicographical order of the current locale.
            Ok(left > right)
        }
        ast::BinaryPredicate::ArithmeticEqualTo => {
            let left =
                arithmetic::expand_and_eval(shell, params, left.value.as_str(), false).await?;
            let right =
                arithmetic::expand_and_eval(shell, params, right.value.as_str(), false).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            Ok(left == right)
        }
        ast::BinaryPredicate::ArithmeticNotEqualTo => {
            let left =
                arithmetic::expand_and_eval(shell, params, left.value.as_str(), false).await?;
            let right =
                arithmetic::expand_and_eval(shell, params, right.value.as_str(), false).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            Ok(left != right)
        }
        ast::BinaryPredicate::ArithmeticLessThan => {
            let left =
                arithmetic::expand_and_eval(shell, params, left.value.as_str(), false).await?;
            let right =
                arithmetic::expand_and_eval(shell, params, right.value.as_str(), false).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            Ok(left < right)
        }
        ast::BinaryPredicate::ArithmeticLessThanOrEqualTo => {
            let left =
                arithmetic::expand_and_eval(shell, params, left.value.as_str(), false).await?;
            let right =
                arithmetic::expand_and_eval(shell, params, right.value.as_str(), false).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            Ok(left <= right)
        }
        ast::BinaryPredicate::ArithmeticGreaterThan => {
            let left =
                arithmetic::expand_and_eval(shell, params, left.value.as_str(), false).await?;
            let right =
                arithmetic::expand_and_eval(shell, params, right.value.as_str(), false).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            Ok(left > right)
        }
        ast::BinaryPredicate::ArithmeticGreaterThanOrEqualTo => {
            let left =
                arithmetic::expand_and_eval(shell, params, left.value.as_str(), false).await?;
            let right =
                arithmetic::expand_and_eval(shell, params, right.value.as_str(), false).await?;

            if shell.options().print_commands_and_arguments {
                shell
                    .trace_command(params, std::format!("[[ {left} {op} {right} ]]"))
                    .await;
            }

            Ok(left >= right)
        }
        // N.B. The "=", "==", and "!=" operators don't compare 2 strings; they check
        // for whether the lefthand operand (a string) is matched by the righthand
        // operand (treated as a shell pattern).
        // TODO(test): implement case-insensitive matching if relevant via shopt options
        // (nocasematch).
        ast::BinaryPredicate::StringExactlyMatchesPattern => {
            let s = expansion::basic_expand_word(shell, params, left).await?;
            let pattern = expansion::basic_expand_pattern(shell, params, right)
                .await?
                .set_extended_globbing(shell.options().extended_globbing)
                .set_case_insensitive(shell.options().case_insensitive_conditionals);

            if shell.options().print_commands_and_arguments {
                let expanded_right = expansion::basic_expand_word(shell, params, right).await?;
                let escaped_right = escape::quote_if_needed(
                    expanded_right.as_str(),
                    escape::QuoteMode::BackslashEscape,
                );
                shell
                    .trace_command(params, std::format!("[[ {s} {op} {escaped_right} ]]"))
                    .await;
            }

            pattern.exactly_matches(s.as_str())
        }
        ast::BinaryPredicate::StringDoesNotExactlyMatchPattern => {
            let s = expansion::basic_expand_word(shell, params, left).await?;
            let pattern = expansion::basic_expand_pattern(shell, params, right)
                .await?
                .set_extended_globbing(shell.options().extended_globbing)
                .set_case_insensitive(shell.options().case_insensitive_conditionals);

            if shell.options().print_commands_and_arguments {
                let expanded_right = expansion::basic_expand_word(shell, params, right).await?;
                let escaped_right = escape::quote_if_needed(
                    expanded_right.as_str(),
                    escape::QuoteMode::BackslashEscape,
                );
                shell
                    .trace_command(params, std::format!("[[ {s} {op} {escaped_right} ]]"))
                    .await;
            }

            let eq = pattern.exactly_matches(s.as_str())?;
            Ok(!eq)
        }
    }
}

pub(crate) fn apply_binary_predicate_to_strs(
    op: &ast::BinaryPredicate,
    left: &str,
    right: &str,
    shell: &Shell<impl extensions::ShellExtensions>,
) -> Result<bool, error::Error> {
    match op {
        ast::BinaryPredicate::FilesReferToSameDeviceAndInodeNumbers => {
            files_refer_to_same_device_and_inode_numbers(shell, left, right)
        }
        ast::BinaryPredicate::LeftFileIsNewerOrExistsWhenRightDoesNot => {
            left_file_is_newer_or_exists_when_right_does_not(shell, left, right)
        }
        ast::BinaryPredicate::LeftFileIsOlderOrDoesNotExistWhenRightDoes => {
            left_file_is_older_or_does_not_exist_when_right_does(shell, left, right)
        }
        ast::BinaryPredicate::LeftSortsBeforeRight => {
            // TODO(test): According to docs, should be lexicographical order of the current locale.
            Ok(left < right)
        }
        ast::BinaryPredicate::LeftSortsAfterRight => {
            // TODO(test): According to docs, should be lexicographical order of the current locale.
            Ok(left > right)
        }
        ast::BinaryPredicate::ArithmeticEqualTo => {
            apply_test_binary_arithmetic_predicate(left, right, |left, right| left == right)
        }
        ast::BinaryPredicate::ArithmeticNotEqualTo => {
            apply_test_binary_arithmetic_predicate(left, right, |left, right| left != right)
        }
        ast::BinaryPredicate::ArithmeticLessThan => {
            apply_test_binary_arithmetic_predicate(left, right, |left, right| left < right)
        }
        ast::BinaryPredicate::ArithmeticLessThanOrEqualTo => {
            apply_test_binary_arithmetic_predicate(left, right, |left, right| left <= right)
        }
        ast::BinaryPredicate::ArithmeticGreaterThan => {
            apply_test_binary_arithmetic_predicate(left, right, |left, right| left > right)
        }
        ast::BinaryPredicate::ArithmeticGreaterThanOrEqualTo => {
            apply_test_binary_arithmetic_predicate(left, right, |left, right| left >= right)
        }
        ast::BinaryPredicate::StringExactlyMatchesPattern => {
            let pattern = patterns::Pattern::from(right)
                .set_extended_globbing(shell.options().extended_globbing)
                .set_case_insensitive(shell.options().case_insensitive_conditionals);

            pattern.exactly_matches(left)
        }
        ast::BinaryPredicate::StringDoesNotExactlyMatchPattern => {
            let pattern = patterns::Pattern::from(right)
                .set_extended_globbing(shell.options().extended_globbing)
                .set_case_insensitive(shell.options().case_insensitive_conditionals);

            let eq = pattern.exactly_matches(left)?;
            Ok(!eq)
        }
        ast::BinaryPredicate::StringExactlyMatchesString => Ok(left == right),
        ast::BinaryPredicate::StringDoesNotExactlyMatchString => Ok(left != right),
        _ => error::unimp("unsupported test binary predicate"),
    }
}

/// `test`'s `-eq` and the rest, on decimal integers as Bash reads them: blanks around
/// them are allowed, anything else, or a number past 64 bits, is `x: integer expected`,
/// status 2. cash took them for false and said nothing.
fn apply_test_binary_arithmetic_predicate(
    left: &str,
    right: &str,
    op: fn(i64, i64) -> bool,
) -> Result<bool, error::Error> {
    let integer = |operand: &str| {
        operand.trim().parse::<i64>().map_err(|_| {
            error::Error::from(error::ErrorKind::TestError(format!(
                "{operand}: integer expected"
            )))
        })
    };
    Ok(op(integer(left)?, integer(right)?))
}

fn left_file_is_older_or_does_not_exist_when_right_does(
    shell: &Shell<impl extensions::ShellExtensions>,
    left: impl AsRef<str>,
    right: impl AsRef<str>,
) -> Result<bool, error::Error> {
    let (l_path, r_path) = (
        shell.absolute_path(Path::new(left.as_ref())),
        shell.absolute_path(Path::new(right.as_ref())),
    );

    match (l_path.metadata(), r_path.metadata()) {
        (Ok(m1), Ok(m2)) => Ok(m1.modified()? < m2.modified()?),
        (Err(_), Ok(_)) => Ok(true),
        _ => Ok(false),
    }
}

fn left_file_is_newer_or_exists_when_right_does_not(
    shell: &Shell<impl extensions::ShellExtensions>,
    left: impl AsRef<str>,
    right: impl AsRef<str>,
) -> Result<bool, error::Error> {
    let (l_path, r_path) = (
        shell.absolute_path(Path::new(left.as_ref())),
        shell.absolute_path(Path::new(right.as_ref())),
    );

    match (l_path.metadata(), r_path.metadata()) {
        (Ok(m1), Ok(m2)) => Ok(m1.modified()? > m2.modified()?),
        (Ok(_), Err(_)) => Ok(true),
        _ => Ok(false),
    }
}

fn files_refer_to_same_device_and_inode_numbers(
    shell: &Shell<impl extensions::ShellExtensions>,
    left: impl AsRef<str>,
    right: impl AsRef<str>,
) -> Result<bool, error::Error> {
    let (l_path, r_path) = (
        shell.absolute_path(Path::new(left.as_ref())),
        shell.absolute_path(Path::new(right.as_ref())),
    );

    if !l_path.readable() || !r_path.readable() {
        return Ok(false);
    }

    Ok(l_path.get_device_and_inode()? == r_path.get_device_and_inode()?)
}
