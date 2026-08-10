use reqwest::Client;
use serde::{Deserialize, Serialize};
use typst_syntax::LinkedNode;
use std::env;
use std::fs;
use typst_syntax::{SyntaxKind, SyntaxNode, parse};

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
        assert_ne!(translated_text,text)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: cargo run -- <input.typ> <output.typ>");
        return Ok(());
    }

    let input_path = &args[1];
    let output_path = &args[2];

    println!("Reading {}...", input_path);
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

    for node in text_nodes {
        let offset = node.offset();
        let len = node.len();
        let original_text = node.text().to_string();

        let translated = translate_text(&client, &original_text).await;

        // Replace the exact slice in the string
        modified_code.replace_range(offset..offset + len, &translated);
    }

    fs::write(output_path, modified_code)?;
    println!("Successfully written translated file to {}", output_path);

    Ok(())
}
