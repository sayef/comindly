//! CloudFormation and SAM templates (also what CDK synthesizes): the queues, topics and buckets
//! they name, and which queues subscribe to which topics. Same facts as Terraform.

use std::path::Path;

use ignore::WalkBuilder;
use serde_json::Value;
use yaml_rust2::{Yaml, YamlLoader};

use crate::facts::{ContractFact, FileFacts, InfraRef};
use crate::model::Symbol;

/// Resource types → (scheme, property holding the name).
const RESOURCES: &[(&str, &str, &str)] = &[
    ("AWS::SQS::Queue", "sqs", "QueueName"),
    ("AWS::SNS::Topic", "sns", "TopicName"),
    ("AWS::S3::Bucket", "s3", "BucketName"),
];

const MAX_BYTES: u64 = 2 << 20;

fn candidate(rel: &str) -> bool {
    let skip = ["node_modules", ".git", "target", ".venv"];
    [".yaml", ".yml", ".json", ".template"]
        .iter()
        .any(|e| rel.ends_with(e))
        && !rel.split('/').any(|d| skip.contains(&d))
}

/// Symbols and facts for every CloudFormation or SAM template in a repo.
pub fn scan(root: &Path, repo: &str) -> Vec<(Symbol, FileFacts)> {
    let rels: Vec<String> = WalkBuilder::new(root)
        .hidden(false)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter(|e| e.metadata().is_ok_and(|m| m.len() <= MAX_BYTES))
        .filter_map(|e| {
            e.path()
                .strip_prefix(root)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .filter(|r| candidate(r))
        .collect();
    files(root, repo, &rels)
}

/// Facts for these files, skipping any that are not templates.
pub fn files(root: &Path, repo: &str, rels: &[String]) -> Vec<(Symbol, FileFacts)> {
    rels.iter()
        .filter(|r| candidate(r))
        .filter_map(|rel| {
            let text = std::fs::read_to_string(root.join(rel)).ok()?;
            let contracts = facts(&template(&text)?);
            (!contracts.is_empty())
                .then(|| crate::infra::file(repo, rel, "cloudformation", contracts))
        })
        .collect()
}

/// The document, when the text is a CloudFormation or SAM template.
fn template(text: &str) -> Option<Value> {
    if !text.contains("AWS::") || !text.contains("Resources") {
        return None;
    }
    let doc = serde_json::from_str::<Value>(text).ok().or_else(|| {
        YamlLoader::load_from_str(text)
            .ok()?
            .into_iter()
            .next()
            .map(|y| json(&y))
    })?;
    doc.get("Resources")?.is_object().then_some(doc)
}

fn json(y: &Yaml) -> Value {
    match y {
        Yaml::String(s) | Yaml::Real(s) => Value::String(s.clone()),
        Yaml::Integer(i) => Value::from(*i),
        Yaml::Boolean(b) => Value::Bool(*b),
        Yaml::Array(a) => Value::Array(a.iter().map(json).collect()),
        Yaml::Hash(h) => Value::Object(
            h.iter()
                .filter_map(|(k, v)| Some((k.as_str()?.to_string(), json(v))))
                .collect(),
        ),
        _ => Value::Null,
    }
}

fn facts(doc: &Value) -> Vec<ContractFact> {
    let Some(resources) = doc.get("Resources").and_then(Value::as_object) else {
        return Vec::new();
    };
    // Logical id → what the resource names.
    let named = |id: &str| -> Option<InfraRef> {
        let r = resources.get(id)?;
        let ty = r.get("Type")?.as_str()?;
        let (_, scheme, prop) = RESOURCES.iter().find(|(t, _, _)| *t == ty)?;
        Some(InfraRef {
            scheme: scheme.to_string(),
            name: name(r.get("Properties")?.get(*prop)?)?,
            logical: false,
        })
    };
    // A reference to a resource (`!Ref T`, `!GetAtt Q.Arn`, `{"Fn::GetAtt": ["Q", "Arn"]}`), or an ARN.
    let target = |v: &Value, scheme: &str| -> Option<InfraRef> {
        let id = match v {
            Value::String(s) if s.starts_with("arn:") => {
                return Some(InfraRef {
                    scheme: scheme.into(),
                    name: s.rsplit(':').next()?.to_string(),
                    logical: false,
                })
            }
            Value::String(s) => s.strip_suffix(".Arn").unwrap_or(s).to_string(),
            Value::Array(a) => a.first()?.as_str()?.to_string(),
            Value::Object(o) => match (o.get("Ref"), o.get("Fn::GetAtt")) {
                (Some(Value::String(s)), _) => s.clone(),
                (_, Some(Value::Array(a))) => a.first()?.as_str()?.to_string(),
                (_, Some(Value::String(s))) => s.split('.').next()?.to_string(),
                _ => return None,
            },
            _ => return None,
        };
        named(&id).filter(|r| r.scheme == scheme)
    };
    let mut out = Vec::new();
    let mut ids: Vec<&String> = resources.keys().collect();
    ids.sort();
    for id in ids {
        let r = &resources[id];
        let props = r.get("Properties");
        if let Some(declares) = named(id) {
            out.push(ContractFact::Infra {
                declares: declares.clone(),
                subscribes: None,
            });
            // A topic's inline subscriptions.
            if declares.scheme == "sns" {
                for s in props
                    .and_then(|p| p.get("Subscription"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(q) = sqs_endpoint(s).and_then(|e| target(e, "sqs")) {
                        out.push(ContractFact::Infra {
                            declares: q,
                            subscribes: Some(declares.clone()),
                        });
                    }
                }
            }
        }
        if r.get("Type").and_then(Value::as_str) == Some("AWS::SNS::Subscription") {
            let Some(p) = props else { continue };
            let queue = sqs_endpoint(p).and_then(|e| target(e, "sqs"));
            let topic = p.get("TopicArn").and_then(|t| target(t, "sns"));
            if let (Some(q), Some(t)) = (queue, topic) {
                out.push(ContractFact::Infra {
                    declares: q,
                    subscribes: Some(t),
                });
            }
        }
    }
    out
}

fn sqs_endpoint(s: &Value) -> Option<&Value> {
    (s.get("Protocol")?.as_str()? == "sqs").then(|| s.get("Endpoint"))?
}

/// A resource name: a literal, or a `!Sub`/`Fn::Sub` template whose parameters become `{}`.
fn name(v: &Value) -> Option<String> {
    let s = match v {
        Value::String(s) => s.as_str(),
        Value::Object(o) => match o.get("Fn::Sub")? {
            Value::String(s) => s.as_str(),
            Value::Array(a) => a.first()?.as_str()?,
            _ => return None,
        },
        _ => return None,
    };
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find("${") {
        out.push_str(&rest[..open]);
        out.push_str("{}");
        rest = rest[open..].find('}').map_or("", |c| &rest[open + c + 1..]);
    }
    out.push_str(rest);
    (!out.is_empty() && out != "{}").then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(c: &[ContractFact], scheme: &str, name: &str, sub: Option<&str>) -> bool {
        c.iter().any(|f| match f {
            ContractFact::Infra {
                declares,
                subscribes,
            } => {
                declares.scheme == scheme
                    && declares.name == name
                    && subscribes.as_ref().map(|s| s.name.as_str()) == sub
            }
            _ => false,
        })
    }

    #[test]
    fn yaml_templates_with_short_tags() {
        let doc = template(
            r#"
AWSTemplateFormatVersion: "2010-09-09"
Resources:
  Events:
    Type: AWS::SNS::Topic
    Properties:
      TopicName: !Sub "billing-${Stage}-events"
  Inbox:
    Type: AWS::SQS::Queue
    Properties:
      QueueName: invoice-inbox
  InboxSubscription:
    Type: AWS::SNS::Subscription
    Properties:
      Protocol: sqs
      TopicArn: !Ref Events
      Endpoint: !GetAtt Inbox.Arn
  Files:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: billing-files
  Unnamed:
    Type: AWS::SQS::Queue
"#,
        )
        .unwrap();
        let c = facts(&doc);
        assert!(has(&c, "sns", "billing-{}-events", None), "{c:?}");
        assert!(
            has(&c, "sqs", "invoice-inbox", Some("billing-{}-events")),
            "{c:?}"
        );
        assert!(has(&c, "s3", "billing-files", None), "{c:?}");
        assert_eq!(c.len(), 4, "{c:?}");
    }

    #[test]
    fn json_templates_with_inline_subscriptions() {
        let doc = template(
            r#"{"Resources": {
  "T": {"Type": "AWS::SNS::Topic", "Properties": {"TopicName": "orders",
        "Subscription": [{"Protocol": "sqs", "Endpoint": {"Fn::GetAtt": ["Q", "Arn"]}}]}},
  "Q": {"Type": "AWS::SQS::Queue", "Properties": {"QueueName": "order-audit"}}}}"#,
        )
        .unwrap();
        let c = facts(&doc);
        assert!(has(&c, "sqs", "order-audit", Some("orders")), "{c:?}");
    }

    #[test]
    fn other_yaml_is_not_a_template() {
        assert!(template("name: app\nresources:\n  cpu: 1\n").is_none());
    }
}
