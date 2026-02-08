use std::sync::LazyLock;

use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use regex::Regex;

#[derive(PartialEq, Debug)]
pub enum RestQuery {
    Something(String),
    Empty,
}
impl RestQuery {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == RestQuery::Empty
    }
}
impl From<&str> for RestQuery {
    fn from(other: &str) -> Self {
        match other {
            "" => RestQuery::Empty,
            other => RestQuery::Something(other.into()),
        }
    }
}
impl From<String> for RestQuery {
    fn from(other: String) -> Self {
        if other.is_empty() {
            RestQuery::Empty
        } else {
            RestQuery::Something(other)
        }
    }
}

pub type QuerySplitter = dyn Fn(&str) -> (String, String, String, String);

// Removing things that are unused for now would require removal of usable code
#[allow(dead_code)]
pub enum ParamGreed {
    Word,
    OptionalWord,
    ToComma,
    Rest,
    Custom(&'static QuerySplitter),
}

pub type Parser<T> = Box<dyn Fn(&str, RestQuery) -> Option<Command<T>>>;

pub enum Command<T> {
    Terminal(T),
    NonTerminal(ParamGreed, Vec<String>, Parser<T>),
}

#[derive(Debug, PartialEq, Clone)]
pub enum ParseError {
    InvalidParameter(String),
    MissingParameters,
    ExtraParameters(String),
}

fn separate_first_word(query: &str) -> (String, String, String, String) {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\W*)(\w*)(\W?)(.*)").unwrap());
    let captures = RE.captures_iter(query).next().unwrap();

    (
        captures[1].into(),
        captures[2].into(),
        captures[3].into(),
        captures[4].into(),
    )
}

fn separate_optional_word(query: &str) -> (String, String, String, String) {
    if query.chars().all(char::is_whitespace) {
        (String::new(), " ".to_string(), String::new(), String::new())
    } else {
        separate_first_word(query)
    }
}

fn separate_until_comma(query: &str) -> (String, String, String, String) {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\W*)([^,]*)(,?)(.*)").unwrap());

    RE.captures_iter(query)
        .next()
        .map(|captures| {
            (
                captures[1].into(),
                captures[2].into(),
                captures[3].into(),
                captures[4].into(),
            )
        })
        .unwrap_or((String::new(), query.into(), String::new(), String::new()))
}

fn split_query(query: &str, greed: &ParamGreed) -> (String, String, String, String) {
    match greed {
        ParamGreed::Word => separate_first_word(query),
        ParamGreed::OptionalWord => separate_optional_word(query),
        ParamGreed::ToComma => separate_until_comma(query),
        ParamGreed::Rest => (
            String::new(),
            query.trim_start().into(),
            String::new(),
            String::new(),
        ),
        ParamGreed::Custom(matcher) => matcher(query),
    }
}

pub fn parse_command<T>(query: &str, command: Command<T>) -> Result<T, ParseError> {
    match command {
        Command::Terminal(val) => match query {
            "" => Ok(val),
            _ => Err(ParseError::ExtraParameters(query.into())),
        },
        Command::NonTerminal(greed, _, parsing_function) => {
            let (_, greed_match, _delim, rest) = split_query(query, &greed);

            match greed_match.as_ref() {
                "" => Err(ParseError::MissingParameters),
                param => match parsing_function(param, rest.clone().into()) {
                    Some(next_command) => parse_command(&rest, next_command),
                    None => Err(ParseError::InvalidParameter(param.into())),
                },
            }
        }
    }
}

pub enum FuzzyError {
    // Indicates that something went wrong when doing fuzzy expansion which
    // lead to trying to do fuzzy matching on a terminal
    ReachedTerminal,
    // The query was expanded based on the suggestions, but the expansion
    // was not a valid command
    MalformedCommand(Vec<String>, String),
    // The fuzzy expander ran out of input and can not make any more suggestions.
    // This should never be returned publicly
    NoMoreInput,
}

pub struct FuzzyOutput {
    pub expanded: String,
    pub suggestions: Result<Vec<(String, Vec<bool>)>, FuzzyError>,
}

fn handle_non_terminal_fuzz<T>(
    previous_query: &str,
    query: &str,
    greed: &ParamGreed,
    suggestions: &[String],
    parser: &Parser<T>,
) -> FuzzyOutput {
    let (leading_whitespace, current_section, delim, rest_query) = split_query(query, greed);
    let rest_query = delim.clone() + &rest_query;

    if leading_whitespace.is_empty() && current_section.is_empty() {
        FuzzyOutput {
            expanded: previous_query.into(),
            suggestions: if previous_query
                .chars()
                .last()
                .is_some_and(char::is_whitespace)
            {
                let s = suggestions
                    .iter()
                    .map(|x| (x.to_string(), vec![false; x.len()]))
                    .collect::<Vec<(String, Vec<bool>)>>();
                Ok(s)
            } else {
                Err(FuzzyError::NoMoreInput)
            },
        }
    } else {
        let expanded_commands = fuzzy_match(suggestions, &current_section);

        let best_expansion = {
            let expansion = expanded_commands
                .first()
                .map_or(&current_section, |(query, _)| query);

            parser(expansion, rest_query.clone().into()).map(|command| (expansion, command))
        };

        let full_query = |expansion| previous_query.to_string() + expansion + &delim;

        match best_expansion {
            Some((expansion, Command::NonTerminal(next_greed, next_suggestions, next_parser))) => {
                let current_query = full_query(expansion);
                let next_result = handle_non_terminal_fuzz(
                    &current_query,
                    &rest_query,
                    &next_greed,
                    &next_suggestions,
                    &next_parser,
                );
                match next_result {
                    FuzzyOutput {
                        suggestions: Err(FuzzyError::NoMoreInput),
                        ..
                    } => {
                        // Return all suggestions for this non-terminal
                        FuzzyOutput {
                            expanded: current_query,
                            suggestions: Ok(expanded_commands),
                        }
                    }
                    future_result => future_result,
                }
            }
            Some((expansion, Command::Terminal(_))) => {
                let current_query = full_query(expansion);
                FuzzyOutput {
                    expanded: current_query,
                    suggestions: Ok(expanded_commands),
                }
            }
            None => {
                let err = Err(FuzzyError::MalformedCommand(
                    suggestions.to_vec(),
                    query.into(),
                ));
                FuzzyOutput {
                    expanded: previous_query.into(),
                    suggestions: err,
                }
            }
        }
    }
}

pub fn expand_command<T>(query: &str, command: Command<T>) -> FuzzyOutput {
    match command {
        Command::NonTerminal(greed, suggestions, parser) => {
            let fuzz_result = handle_non_terminal_fuzz("", query, &greed, &suggestions, &parser);
            match fuzz_result {
                FuzzyOutput {
                    expanded,
                    suggestions: Err(FuzzyError::NoMoreInput),
                } => {
                    let suggestion_matches = suggestions
                        .iter()
                        .cloned()
                        .map(|s| {
                            let falses = (0..s.len()).map(|_| false).collect();
                            (s, falses)
                        })
                        .collect();

                    FuzzyOutput {
                        expanded,
                        suggestions: Ok(suggestion_matches),
                    }
                }
                other => other,
            }
        }
        Command::Terminal(_) => FuzzyOutput {
            expanded: String::new(),
            suggestions: Err(FuzzyError::ReachedTerminal),
        },
    }
}

fn fuzzy_match(alternatives: &[String], query: &str) -> Vec<(String, Vec<bool>)> {
    let mut with_scores = alternatives
        .iter()
        .map(|option| {
            let (score, matches) = fuzzy_score(option, query);
            (option, score, matches)
        })
        .collect::<Vec<_>>();

    with_scores.sort_by_key(|(option, score, _)| (-*score, option.len()));

    with_scores
        .into_iter()
        .map(|(value, _, matches)| (value.clone(), matches))
        .collect()
}

fn fuzzy_score(line: &str, query: &str) -> (i64, Vec<bool>) {
    static MATCHER: LazyLock<SkimMatcherV2> = LazyLock::new(SkimMatcherV2::default);
    let (score, indices) = MATCHER.fuzzy_indices(line, query).unwrap_or_default();

    let mut matches = vec![false; line.len()];
    indices.iter().for_each(|&i| matches[i] = true);
    (score, matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, PartialEq)]
    enum CommandOutputs {
        NoParams,
        OptionalParam(Option<String>),
        OneParam(String),
        ParsedParam(i32),
        Two(String, String),
    }

    fn get_parser() -> Command<CommandOutputs> {
        fn single_word(
            suggestions: Vec<String>,
            rest_command: Box<dyn Fn(&str) -> Option<Command<CommandOutputs>>>,
        ) -> Option<Command<CommandOutputs>> {
            Some(Command::NonTerminal(
                ParamGreed::Word,
                suggestions,
                Box::new(move |query, _| rest_command(query)),
            ))
        }
        fn optional_word(
            suggestions: Vec<String>,
            rest_command: Box<dyn Fn(&str) -> Option<Command<CommandOutputs>>>,
        ) -> Option<Command<CommandOutputs>> {
            Some(Command::NonTerminal(
                ParamGreed::OptionalWord,
                suggestions,
                Box::new(move |query, _| rest_command(query)),
            ))
        }
        fn single_comma_separation(
            suggestions: Vec<String>,
            rest_command: Box<dyn Fn(&str) -> Option<Command<CommandOutputs>>>,
        ) -> Option<Command<CommandOutputs>> {
            Some(Command::NonTerminal(
                ParamGreed::ToComma,
                suggestions,
                Box::new(move |query, _| rest_command(query)),
            ))
        }

        Command::NonTerminal(
            ParamGreed::Word,
            vec!["noparams".into(), "oneparam".into(), "parsedparam".into()],
            Box::new(|query, _| {
                let multi_comma = single_comma_separation(
                    vec![],
                    Box::new(|first| {
                        let first = first.to_string();
                        Some(Command::NonTerminal(
                            ParamGreed::ToComma,
                            vec![],
                            Box::new(move |second, _| {
                                Some(Command::Terminal(CommandOutputs::Two(
                                    first.clone(),
                                    second.into(),
                                )))
                            }),
                        ))
                    }),
                );
                match query {
                    "noparams" => Some(Command::Terminal(CommandOutputs::NoParams)),
                    "oneparam" => single_word(
                        vec![],
                        Box::new(|word| {
                            Some(Command::Terminal(CommandOutputs::OneParam(word.into())))
                        }),
                    ),
                    "optionalparam" => optional_word(
                        vec![],
                        Box::new(|word| {
                            Some(Command::Terminal(CommandOutputs::OptionalParam(
                                if word == " " { None } else { Some(word.into()) },
                            )))
                        }),
                    ),
                    "parsedparam" => single_word(
                        vec![],
                        Box::new(|word| {
                            word.parse::<i32>()
                                .map(|int| Command::Terminal(CommandOutputs::ParsedParam(int)))
                                .ok()
                        }),
                    ),
                    "singlecomma" => single_comma_separation(
                        vec![],
                        Box::new(|word| {
                            word.parse::<i32>()
                                .map(|int| Command::Terminal(CommandOutputs::ParsedParam(int)))
                                .ok()
                        }),
                    ),
                    "multicomma" => multi_comma,
                    _ => None,
                }
            }),
        )
    }

    #[test]
    fn basic_parsing_test() {
        let parser = get_parser();

        let result = parse_command("noparams", parser);
        assert_eq!(result, Ok(CommandOutputs::NoParams));
    }

    #[test]
    fn parsing_with_params_works() {
        let parser = get_parser();
        let result = parse_command("oneparam test", parser);

        assert_eq!(result, Ok(CommandOutputs::OneParam("test".into())));
    }

    #[test]
    fn parsing_with_parsed_param_works() {
        let parser = get_parser();

        let result = parse_command("parsedparam 5", parser);

        assert_eq!(result, Ok(CommandOutputs::ParsedParam(5)));
    }
    #[test]
    fn parsing_with_commas_works_with_missing_trailing_comma() {
        let parser = get_parser();

        let result = parse_command("singlecomma 5", parser);

        assert_eq!(result, Ok(CommandOutputs::ParsedParam(5)));
    }

    #[test]
    fn parsing_with_multiple_commas_works() {
        let parser = get_parser();

        let result = parse_command("multicomma yolo, swag", parser);

        assert_eq!(
            result,
            Ok(CommandOutputs::Two("yolo".into(), "swag".into()))
        );
    }

    #[test]
    fn parsing_optional_word_works() {
        let parser = get_parser();

        let result = parse_command("optionalparam yolo", parser);
        assert_eq!(
            result,
            Ok(CommandOutputs::OptionalParam(Some("yolo".into())))
        );
    }

    #[test]
    fn parsing_optional_without_word_works() {
        let parser = get_parser();

        let result = parse_command("optionalparam", parser);
        assert_eq!(result, Ok(CommandOutputs::OptionalParam(None)));
    }

    macro_rules! test_order {
        ($list:expr, $query:expr, $expected:expr) => {
            assert_eq!(
                fuzzy_match(
                    &$list.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                    $query
                )
                .into_iter()
                .map(|m| m.0.clone())
                .collect::<Vec<_>>(),
                $expected.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            )
        };
    }

    #[test]
    fn shorter_early_matching_string_is_better_than_longer() {
        test_order!(
            ["cpu_test_harness", "cpu"],
            "cpu",
            ["cpu", "cpu_test_harness"]
        );
    }

    #[test]
    fn separate_first_word_handles_empty_string() {
        let result = separate_first_word("");
        assert_eq!(
            result,
            (String::new(), String::new(), String::new(), String::new())
        );
    }

    #[test]
    fn separate_first_word_handles_whitespace_only() {
        let result = separate_first_word("   ");
        assert_eq!(
            result,
            ("   ".into(), String::new(), String::new(), String::new())
        );
    }

    #[test]
    fn separate_first_word_handles_single_word() {
        let result = separate_first_word("test");
        assert_eq!(
            result,
            (String::new(), "test".into(), String::new(), String::new())
        );
    }

    #[test]
    fn separate_first_word_handles_word_with_trailing() {
        let result = separate_first_word("test hello");
        assert_eq!(
            result,
            (String::new(), "test".into(), " ".into(), "hello".into())
        );
    }

    #[test]
    fn separate_until_comma_handles_empty_string() {
        let result = separate_until_comma("");
        assert_eq!(
            result,
            (String::new(), String::new(), String::new(), String::new())
        );
    }

    #[test]
    fn separate_until_comma_handles_no_comma() {
        let result = separate_until_comma("test");
        assert_eq!(
            result,
            (String::new(), "test".into(), String::new(), String::new())
        );
    }

    #[test]
    fn separate_until_comma_handles_with_comma() {
        let result = separate_until_comma("first,second");
        assert_eq!(
            result,
            (String::new(), "first".into(), ",".into(), "second".into())
        );
    }

    #[test]
    fn fuzzy_score_handles_empty_query() {
        let result = fuzzy_score("test", "");
        assert_eq!(result.0, 0);
        assert_eq!(result.1, vec![false, false, false, false]);
    }

    #[test]
    fn fuzzy_score_handles_empty_line() {
        let result = fuzzy_score("", "query");
        assert_eq!(result.0, 0);
        assert_eq!(result.1, Vec::<bool>::new());
    }

    #[test]
    fn fuzzy_score_handles_exact_match() {
        let result = fuzzy_score("test", "test");
        assert!(result.0 > 0);
        assert_eq!(result.1, vec![true, true, true, true]);
    }

    #[test]
    fn fuzzy_match_handles_empty_alternatives() {
        let result = fuzzy_match(&[], "query");
        assert_eq!(result, vec![]);
    }

    #[test]
    fn fuzzy_match_handles_empty_query() {
        let alternatives = vec!["test".to_string(), "other".to_string()];
        let result = fuzzy_match(&alternatives, "");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn parse_command_rejects_extra_parameters_for_terminal() {
        let command = Command::Terminal(CommandOutputs::NoParams);
        let result = parse_command("extra stuff", command);
        assert_eq!(
            result,
            Err(ParseError::ExtraParameters("extra stuff".into()))
        );
    }

    #[test]
    fn parse_command_handles_missing_parameters() {
        let parser = get_parser();
        let result = parse_command("oneparam", parser);
        assert_eq!(result, Err(ParseError::MissingParameters));
    }

    #[test]
    fn parse_command_handles_invalid_parameter() {
        let parser = get_parser();
        let result = parse_command("parsedparam notanumber", parser);
        assert_eq!(
            result,
            Err(ParseError::InvalidParameter("notanumber".into()))
        );
    }

    #[test]
    fn expand_command_handles_terminal() {
        let command = Command::Terminal(CommandOutputs::NoParams);
        let result = expand_command("", command);
        assert_eq!(result.expanded, "");
        assert!(matches!(
            result.suggestions,
            Err(FuzzyError::ReachedTerminal)
        ));
    }

    #[test]
    fn expand_command_handles_empty_query() {
        let parser = get_parser();
        let result = expand_command("", parser);
        assert_eq!(result.expanded, "");
        assert!(result.suggestions.is_ok());
    }
}
