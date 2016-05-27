//! A script compiler
//!
//! This compiler take untrusted code (`Script<UncheckedCtx>`) and
//! performs the following transformations and checks:
//!
//! - Ensure that the `Script` has at least one `Rule`.
//! - Ensure that each `Rule` has at least one `Match`.
//! - Ensure that each `Rule` has at least one `Statement`.
//! - Ensure that each `Match` has at least one `source`.
//! - Ensure that each `Statement` has at least one `destination`.
//! - Ensure that in each `Match`, the type of `range` matches
//!   the `kind`.
//! - Ensure that in each `Statement`, the type of `value` matches
//!   the `kind`.
//! - Transform each `Match` to make sure that the kind of the
//!   `source` matches the `kind`, even if devices change.
//! - Transform each `Statement` to make sure that the kind of the
//!   `destination` matches the `kind`, even if devices change.

use ast::{ Script, Rule, Statement, Match, Context, UncheckedCtx };
use util::*;

use foxbox_taxonomy::api::API;
use foxbox_taxonomy::util::Exactly;
use foxbox_taxonomy::values::Duration;

use transformable_channels::mpsc::*;

use std::fmt::{ Debug, Formatter, Error as FmtError };
use std::marker::PhantomData;

/// The environment in which the code is meant to be executed.  This
/// can typically be instantiated either with actual bindings to
/// devices, or with a unit-testing framework. // FIXME: Move this to run.rs
pub trait ExecutableDevEnv: Send {
    type WatchGuard;
    type API: API<WatchGuard = Self::WatchGuard>;

    /// Return a handle to the API.
    fn api(&self) -> &Self::API;

    /// A guard returned by `start_timer`. When the guard is dropped, the timer is cancelled.
    type TimerGuard;
    fn start_timer(&self, duration: Duration, timer: Box<ExtSender<()>>) -> Self::TimerGuard;
}
impl<W, A, T> Debug for ExecutableDevEnv<WatchGuard=W, API=A, TimerGuard=T> {
    fn fmt(&self, _: &mut Formatter) -> Result<(), FmtError> {
        Ok(())
    }
}

///
/// # Precompilation
///

#[derive(Debug)]
pub struct CompiledCtx<Env>  {
    phantom: PhantomData<Env>,
}


impl<Env> Context for CompiledCtx<Env>  {
}

#[derive(Clone, Debug, Serialize)]
pub enum SourceError {
    /// The source doesn't define any rule.
    NoRule,

    /// A rule doesn't have any statements.
    NoStatement,

    /// A rule doesn't have any condition.
    NoMatch,

    /// A match doesn't have any source.
    NoMatchSource,

    /// A statement doesn't have any destination.
    NoStatementDestination,
}

#[derive(Clone, Debug, Serialize)]
pub enum TypeError {
    /// The range cannot be typed.
    InvalidRange,

    /// The range has one type but this type is incompatible with the
    /// kind of the `Match`.
    KindAndRangeDoNotAgree,

    /// The value has one type but this type is incompatible with the
    /// kind of the `Statement`.
    KindAndValueDoNotAgree,
}

#[derive(Clone, Debug, Serialize)]
pub enum Error {
    SourceError(SourceError),
    TypeError(TypeError),
}

pub struct Compiler<Env> where Env: ExecutableDevEnv {
    phantom: PhantomData<Env>,
}

impl<Env> Compiler<Env> where Env: ExecutableDevEnv {
    pub fn new() -> Result<Self, Error> {
        Ok(Compiler {
            phantom: PhantomData
        })
    }

    /// Attempt to compile a script.
    pub fn compile(&self, script: Script<UncheckedCtx>)
                   -> Result<Script<CompiledCtx<Env>>, Error> {
        self.compile_script(script)
    }

    fn compile_script(&self, script: Script<UncheckedCtx>) -> Result<Script<CompiledCtx<Env>>, Error>
    {
        if script.rules.len() == 0 {
            return Err(Error::SourceError(SourceError::NoRule));
        }
        let rules = try!(map(script.rules, |rule| {
            self.compile_rule(rule)
        }));
        Ok(Script {
            name: script.name,
            rules: rules,
            phantom: PhantomData
        })
    }

    fn compile_rule(&self, trigger: Rule<UncheckedCtx>) -> Result<Rule<CompiledCtx<Env>>, Error>
    {
        if trigger.execute.len() == 0 {
            return Err(Error::SourceError(SourceError::NoStatement));
        }
        if trigger.conditions.len() == 0 {
            return Err(Error::SourceError(SourceError::NoMatch));
        }
        let conditions = try!(map(trigger.conditions, |match_| {
            self.compile_match(match_)
        }));
        let execute = try!(map(trigger.execute, |statement| {
            self.compile_statement(statement)
        }));
        Ok(Rule {
            conditions: conditions,
            execute: execute,
            phantom: PhantomData
        })
    }

    fn compile_match(&self, match_: Match<UncheckedCtx>) -> Result<Match<CompiledCtx<Env>>, Error>
    {
        if match_.source.len() == 0 {
            return Err(Error::SourceError(SourceError::NoMatchSource));
        }
        let source = match_.source
            .iter()
            .map(|input| input.clone()
                 .with_feature(&match_.feature)
                 .with_supports_watch(Exactly::Exactly(true)))
            .collect();
        Ok(Match {
            source: source,
            feature: match_.feature,
            range: match_.range,
            duration: match_.duration,
            phantom: PhantomData
        })
    }

    fn compile_statement(&self, statement: Statement<UncheckedCtx>) -> Result<Statement<CompiledCtx<Env>>, Error>
    {
        if statement.destination.len() == 0 {
            return Err(Error::SourceError(SourceError::NoStatementDestination));
        }
        let destination = statement.destination
            .iter()
            .map(|output| output.clone()
                 .with_feature(&statement.feature))
            .collect();
        Ok(Statement {
            destination: destination,
            value: statement.value,
            feature: statement.feature,
            phantom: PhantomData
        })
    }
}
