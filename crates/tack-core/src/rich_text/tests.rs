use super::to_plain_text;

#[test]
fn a_description_without_the_editors_markup_is_returned_as_is() {
    for plain in [
        "Fix the Makefile.\n\n- keep `make test`\n- 1 < 2 && 3 > 2",
        "a &amp; b stays literal: nobody wrote HTML",
        "",
    ] {
        assert_eq!(to_plain_text(plain), plain);
    }
}

#[test]
fn paragraphs_and_the_divs_a_browser_inserts_on_enter_become_lines() {
    assert_eq!(
        to_plain_text("<p>línea 1</p><p>línea 2</p><p>línea 3</p>"),
        "línea 1\n\nlínea 2\n\nlínea 3"
    );
    assert_eq!(
        to_plain_text("línea 1<div>línea 2</div><div><br></div><div>línea 3</div>"),
        "línea 1\n\nlínea 2\n\nlínea 3"
    );
    assert_eq!(
        to_plain_text("uno<br>dos<br><br>tres<br>"),
        "uno\ndos\n\ntres"
    );
}

#[test]
fn lists_keep_their_markers_numbers_and_nesting() {
    let html = "<p>Criterios:</p><ul><li>el <strong>Makefile</strong> compila</li>\
                <li>los tests pasan<ul><li>unit</li><li>e2e</li></ul></li></ul>\
                <ol><li>primero</li><li>segundo</li></ol>";
    assert_eq!(
        to_plain_text(html),
        "Criterios:\n\n- el **Makefile** compila\n- los tests pasan\n  - unit\n  - e2e\n\n1. primero\n2. segundo"
    );
}

#[test]
fn headings_quotes_code_and_links_read_as_markdown() {
    let html = "<h2>Contexto</h2><blockquote>dicho por <em>Prill</em></blockquote>\
                <p>Corre <code>make test</code> y lee <a href=\"https://example.test/x\">la guía</a>.</p>\
                <pre>make test\n  make lint</pre>";
    assert_eq!(
        to_plain_text(html),
        "## Contexto\n\n> dicho por *Prill*\n\nCorre `make test` y lee la guía (https://example.test/x).\n\n```\nmake test\n  make lint\n```"
    );
}

#[test]
fn entities_are_decoded_and_whitespace_collapsed() {
    assert_eq!(
        to_plain_text("<p>a&nbsp;&amp;   b &lt;c&gt; &#233;&#x00e9; &bogus;</p>"),
        "a & b <c> éé &bogus;"
    );
    assert_eq!(
        to_plain_text("<ul>\n  <li>uno</li>\n  <li>dos</li>\n</ul>"),
        "- uno\n- dos"
    );
}

#[test]
fn script_and_style_content_never_reaches_the_agent() {
    assert_eq!(
        to_plain_text(
            "<p>ok</p><script>alert(1)</script><style>p{}</style><!-- note --><p>fin</p>"
        ),
        "ok\n\nfin"
    );
}
