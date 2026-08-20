use clap::Parser;
use inquire::{Select, Text};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use typst_syntax::LinkedNode;
use typst_syntax::{SyntaxKind, SyntaxNode, parse};

#[derive(Parser, Debug)]
#[command(author, version, about = "Übersetzt Typst-Dateien", long_about = None)]
struct Args {
    /// Pfad zur Eingabedatei
    input: PathBuf,

    /// Pfad zur Ausgabedatei
    output: PathBuf,

    /// Anzahl der Alternativen (optional, Standardwert: 1)
    #[arg(long, default_value_t = 1)]
    alternatives: usize,

    /// Optionales Flag (z. B. --verbose oder -v)
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Serialize)]
struct TranslateRequestAlt<'a> {
    q: &'a str,
    source: &'a str,
    target: &'a str,
    format: &'a str,
    alternatives: usize,
    api_key: &'a str,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")] // Wandelt translatedText automatisch aus translated_text um
struct TranslateResponseAlt {
    translated_text: String,
    #[serde(default)]
    alternatives: Vec<String>,
}

#[derive(Serialize)]
struct TranslateRequest<'a> {
    q: &'a str,
    source: &'a str,
    target: &'a str,
    format: &'a str,
}

#[derive(Deserialize)]
struct TranslateResponse {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

// Translate a single text string via local LibreTranslate HTTP server
async fn translate_text(client: &Client, text: &str) -> String {
    let trimmed = text.trim();
    // Skip empty strings or whitespace
    if trimmed.is_empty() {
        return text.to_string();
    }

    let payload = TranslateRequest {
        q: text,
        source: "de",
        target: "en",
        format: "text",
    };

    match client
        .post("http://localhost:5000/translate")
        .json(&payload)
        .send()
        .await
    {
        Ok(res) => {
            if let Ok(json) = res.json::<TranslateResponse>().await {
                json.translated_text
            } else {
                text.to_string()
            }
        }
        Err(_) => text.to_string(),
    }
}

async fn translate_text_with_alternatives(
    client: &Client,
    text: &str,
    num_alternatives: usize,
) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return vec![text.to_string()];
    }

    let payload = TranslateRequestAlt {
        q: text,
        source: "de",
        target: "en",
        format: "text",
        alternatives: num_alternatives,
        api_key: "",
    };

    let res = client
        .post("http://localhost:5000/translate")
        .json(&payload)
        .send()
        .await;

    match res {
        Ok(response) => {
            if let Ok(data) = response.json::<TranslateResponseAlt>().await {
                // Hauptübersetzung an erster Stelle, danach die Alternativen
                let mut choices = vec![data.translated_text];
                choices.extend(data.alternatives);
                choices
            } else {
                vec![text.to_string()]
            }
        }
        Err(_) => vec![text.to_string()],
    }
}

fn select_best_translation(original_text: &str, choices: Vec<String>) -> String {
    if choices.len() <= 1 {
        return choices
            .into_iter()
            .next()
            .unwrap_or_else(|| original_text.to_string());
    }

    println!("\nOriginal: \"{}\"", original_text);

    // Auswahloptionen vorbereiten
    let mut options = choices.clone();
    options.push("[ Eigenen Text eingeben ]".to_string());

    let selection = Select::new("Wähle die beste Übersetzung:", options).prompt();

    match selection {
        Ok(choice) => {
            if choice == "[ Eigenen Text eingeben ]" {
                Text::new("Gib deine eigene Übersetzung ein:")
                    .prompt()
                    .unwrap_or_else(|_| choices[0].clone())
            } else {
                choice
            }
        }
        Err(_) => {
            // Fallback auf die erste Option bei Abbrechen (z. B. Strg+C)
            choices[0].clone()
        }
    }
}

// Recursively find nodes that represent raw human text
fn collect_text_nodes<'a>(node: LinkedNode<'a>, text_nodes: &mut Vec<LinkedNode<'a>>) {
    // SyntaxKind::Text is the exact AST node type for body text inside markup/content blocks
    if node.kind() == SyntaxKind::Text {
        text_nodes.push(node);
        return;
    }

    // Skip code expressions, raw code blocks, and math equations entirely
    if matches!(
        node.kind(),
        SyntaxKind::Code
            | SyntaxKind::Raw
            | SyntaxKind::Equation
            | SyntaxKind::LineComment
            | SyntaxKind::BlockComment
    ) {
        return;
    }

    // Continue traversing child nodes
    for child in node.children() {
        collect_text_nodes(child, text_nodes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_translate_text() {
        let text = "Dieser Text sollte auf Englisch zu lesen sein";
        let client = Client::new();
        let translated_text: String = translate_text(&client, &text).await;
        assert_ne!(translated_text, text)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let input_path = &args.input;
    let output_path = &args.output;

    println!("Reading {}...", input_path.display());
    let source_code = fs::read_to_string(input_path)?;

    // Parse into Typst's official Concrete Syntax Tree (CST)
    let root: SyntaxNode = parse(&source_code);
    let root_linked = LinkedNode::new(&root);

    let mut text_nodes = Vec::new();
    collect_text_nodes(root_linked, &mut text_nodes);

    println!("Found {} translatable text nodes.", text_nodes.len());

    let client = Client::new();
    let mut modified_code = source_code.clone();

    // Sort nodes in REVERSE by byte offset so replacing text doesn't corrupt previous positions
    text_nodes.sort_by_key(|n| (*n).offset());
    text_nodes.reverse();

    if args.alternatives == 1 {
        for node in text_nodes {
            let offset = node.offset();
            let len = node.len();
            let original_text = node.text().to_string();

            let translated = translate_text(&client, &original_text).await;

            // Replace the exact slice in the string
            modified_code.replace_range(offset..offset + len, &translated);
        }
    } else if args.alternatives >= 1 {
        for node in text_nodes {
            let offset = node.offset();
            let len = node.len();
            let original_text = node.text().to_string();
            let alternatives_count = args.alternatives;

            let options =
                translate_text_with_alternatives(&client, &original_text, alternatives_count).await;
            let final_translation = select_best_translation(&original_text, options);
            // Replace the exact slice in the string
            modified_code.replace_range(offset..offset + len, &final_translation);
        }
    }

    fs::write(output_path, modified_code)?;
    println!(
        "Successfully written translated file to {}",
        output_path.display()
    );

    Ok(())
}
