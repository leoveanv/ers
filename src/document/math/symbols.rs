// #NOTE: Symbol and script tables map KaTeX/RaTeX command names and ASCII characters to Unicode equivalents.

pub(super) fn tex_symbol(token: &str) -> Option<&'static str> {
    match token {
        // Lowercase Greek
        r"\alpha" => Some("α"),
        r"\beta" => Some("β"),
        r"\gamma" => Some("γ"),
        r"\delta" => Some("δ"),
        r"\epsilon" => Some("ϵ"),
        r"\varepsilon" => Some("ε"),
        r"\zeta" => Some("ζ"),
        r"\eta" => Some("η"),
        r"\theta" => Some("θ"),
        r"\vartheta" => Some("ϑ"),
        r"\iota" => Some("ι"),
        r"\kappa" => Some("κ"),
        r"\varkappa" => Some("ϰ"),
        r"\lambda" => Some("λ"),
        r"\mu" => Some("μ"),
        r"\nu" => Some("ν"),
        r"\xi" => Some("ξ"),
        r"\pi" => Some("π"),
        r"\varpi" => Some("ϖ"),
        r"\rho" => Some("ρ"),
        r"\varrho" => Some("ϱ"),
        r"\sigma" => Some("σ"),
        r"\varsigma" => Some("ς"),
        r"\tau" => Some("τ"),
        r"\upsilon" => Some("υ"),
        r"\phi" => Some("ϕ"),
        r"\varphi" => Some("φ"),
        r"\chi" => Some("χ"),
        r"\psi" => Some("ψ"),
        r"\omega" => Some("ω"),

        // Uppercase Greek
        r"\Gamma" | r"\varGamma" => Some("Γ"),
        r"\Delta" | r"\varDelta" => Some("Δ"),
        r"\Theta" | r"\varTheta" => Some("Θ"),
        r"\Lambda" | r"\varLambda" => Some("Λ"),
        r"\Xi" | r"\varXi" => Some("Ξ"),
        r"\Pi" | r"\varPi" => Some("Π"),
        r"\Sigma" | r"\varSigma" => Some("Σ"),
        r"\Upsilon" | r"\varUpsilon" => Some("Υ"),
        r"\Phi" | r"\varPhi" => Some("Φ"),
        r"\Psi" | r"\varPsi" => Some("Ψ"),
        r"\Omega" | r"\varOmega" => Some("Ω"),

        // Operators and arithmetic
        r"\times" => Some("×"),
        r"\cdot" | r"\cdotp" | r"\centerdot" => Some("·"),
        r"\div" => Some("÷"),
        r"\pm" => Some("±"),
        r"\mp" => Some("∓"),
        r"\ast" => Some("∗"),
        r"\star" => Some("⋆"),
        r"\circ" => Some("∘"),
        r"\bullet" => Some("•"),
        r"\oplus" => Some("⊕"),
        r"\ominus" => Some("⊖"),
        r"\otimes" => Some("⊗"),
        r"\oslash" => Some("⊘"),
        r"\odot" => Some("⊙"),
        r"\dagger" => Some("†"),
        r"\ddagger" => Some("‡"),
        r"\setminus" | r"\smallsetminus" => Some("∖"),
        r"\wedge" | r"\land" => Some("∧"),
        r"\vee" | r"\lor" => Some("∨"),
        r"\cap" => Some("∩"),
        r"\cup" => Some("∪"),
        r"\uplus" => Some("⊎"),
        r"\sqcap" => Some("⊓"),
        r"\sqcup" => Some("⊔"),

        // Big operators
        r"\sum" => Some("∑"),
        r"\prod" => Some("∏"),
        r"\coprod" => Some("∐"),
        r"\int" | r"\intop" => Some("∫"),
        r"\iint" => Some("∬"),
        r"\iiint" => Some("∭"),
        r"\oint" => Some("∮"),
        r"\oiint" => Some("∯"),
        r"\oiiint" => Some("∰"),
        r"\bigcup" => Some("⋃"),
        r"\bigcap" => Some("⋂"),
        r"\bigvee" => Some("⋁"),
        r"\bigwedge" => Some("⋀"),
        r"\bigoplus" => Some("⨁"),
        r"\bigotimes" => Some("⨂"),
        r"\bigodot" => Some("⨀"),

        // Relations
        r"\le" | r"\leq" | r"\leqslant" => Some("≤"),
        r"\ge" | r"\geq" | r"\geqslant" => Some("≥"),
        r"\neq" | r"\ne" => Some("≠"),
        r"\equiv" => Some("≡"),
        r"\approx" => Some("≈"),
        r"\sim" => Some("∼"),
        r"\simeq" => Some("≃"),
        r"\cong" => Some("≅"),
        r"\propto" => Some("∝"),
        r"\ll" => Some("≪"),
        r"\gg" => Some("≫"),
        r"\prec" => Some("≺"),
        r"\succ" => Some("≻"),
        r"\preceq" => Some("⪯"),
        r"\succeq" => Some("⪰"),
        r"\subset" => Some("⊂"),
        r"\supset" => Some("⊃"),
        r"\subseteq" => Some("⊆"),
        r"\supseteq" => Some("⊇"),
        r"\nsubseteq" => Some("⊈"),
        r"\nsupseteq" => Some("⊉"),
        r"\in" => Some("∈"),
        r"\notin" => Some("∉"),
        r"\ni" | r"\owns" => Some("∋"),
        r"\perp" => Some("⊥"),
        r"\parallel" => Some("∥"),
        r"\mid" | r"\vert" | r"\|" => Some("∣"),
        r"\nmid" => Some("∤"),
        r"\models" => Some("⊨"),
        r"\vdash" => Some("⊢"),
        r"\dashv" => Some("⊣"),
        r"\asymp" => Some("≍"),
        r"\doteq" => Some("≐"),
        r"\triangleq" => Some("≜"),
        r"\coloneqq" => Some("≔"),
        r"\eqqcolon" => Some("≕"),

        // Arrows
        r"\to" | r"\rightarrow" => Some("→"),
        r"\leftarrow" | r"\gets" => Some("←"),
        r"\leftrightarrow" => Some("↔"),
        r"\Rightarrow" => Some("⇒"),
        r"\Leftarrow" => Some("⇐"),
        r"\Leftrightarrow" | r"\iff" => Some("⇔"),
        r"\longrightarrow" => Some("⟶"),
        r"\longleftarrow" => Some("⟵"),
        r"\longleftrightarrow" => Some("⟷"),
        r"\Longrightarrow" | r"\implies" => Some("⟹"),
        r"\Longleftarrow" | r"\impliedby" => Some("⟸"),
        r"\Longleftrightarrow" => Some("⟺"),
        r"\mapsto" => Some("↦"),
        r"\longmapsto" => Some("⟼"),
        r"\uparrow" => Some("↑"),
        r"\downarrow" => Some("↓"),
        r"\updownarrow" => Some("↕"),
        r"\Uparrow" => Some("⇑"),
        r"\Downarrow" => Some("⇓"),
        r"\nearrow" => Some("↗"),
        r"\searrow" => Some("↘"),
        r"\swarrow" => Some("↙"),
        r"\nwarrow" => Some("↖"),
        r"\hookrightarrow" => Some("↪"),
        r"\hookleftarrow" => Some("↩"),
        r"\rightharpoonup" => Some("⇀"),
        r"\rightharpoondown" => Some("⇁"),
        r"\rightleftharpoons" => Some("⇌"),

        // Logic, calculus, miscellaneous
        r"\infty" => Some("∞"),
        r"\partial" => Some("∂"),
        r"\nabla" => Some("∇"),
        r"\forall" => Some("∀"),
        r"\exists" => Some("∃"),
        r"\nexists" => Some("∄"),
        r"\emptyset" | r"\varnothing" => Some("∅"),
        r"\neg" | r"\lnot" => Some("¬"),
        r"\top" => Some("⊤"),
        r"\bot" => Some("⊥"),
        r"\aleph" => Some("ℵ"),
        r"\hbar" | r"\hslash" => Some("ℏ"),
        r"\ell" => Some("ℓ"),
        r"\wp" => Some("℘"),
        r"\Re" => Some("ℜ"),
        r"\Im" => Some("ℑ"),
        r"\imath" => Some("ı"),
        r"\jmath" => Some("ȷ"),
        r"\angle" => Some("∠"),
        r"\triangle" => Some("△"),
        r"\square" | r"\Box" => Some("□"),
        r"\diamond" => Some("◇"),
        r"\dots" | r"\ldots" | r"\dotsc" | r"\dotsb" | r"\dotsi" => Some("…"),
        r"\cdots" => Some("⋯"),
        r"\vdots" => Some("⋮"),
        r"\ddots" => Some("⋱"),
        r"\prime" => Some("′"),
        r"\degree" => Some("°"),
        r"\therefore" => Some("∴"),
        r"\because" => Some("∵"),
        r"\checkmark" => Some("✓"),
        r"\flat" => Some("♭"),
        r"\natural" => Some("♮"),
        r"\sharp" => Some("♯"),
        r"\clubsuit" => Some("♣"),
        r"\diamondsuit" => Some("♢"),
        r"\heartsuit" => Some("♡"),
        r"\spadesuit" => Some("♠"),

        // Delimiters and escaped punctuation
        r"\langle" => Some("⟨"),
        r"\rangle" => Some("⟩"),
        r"\lfloor" => Some("⌊"),
        r"\rfloor" => Some("⌋"),
        r"\lceil" => Some("⌈"),
        r"\rceil" => Some("⌉"),
        r"\{" | r"\lbrace" => Some("{"),
        r"\}" | r"\rbrace" => Some("}"),
        r"\lbrack" => Some("["),
        r"\rbrack" => Some("]"),
        r"\backslash" => Some("\\"),
        r"\%" => Some("%"),
        r"\$" => Some("$"),
        r"\&" => Some("&"),
        r"\#" => Some("#"),
        r"\_" => Some("_"),

        // Spacing
        r"\," | r"\thinspace" | r"\:" | r"\;" | r"\ " | r"\enspace" | r"\quad" | r"\qquad" => {
            Some(" ")
        }
        r"\!" => Some(""),

        _ => None,
    }
}

pub(super) fn delimiter_symbol(delim: &str) -> &str {
    if delim == "." {
        return "";
    }
    match delim {
        r"\|" | r"\Vert" => "‖",
        r"\vert" => "|",
        _ => tex_symbol(delim).unwrap_or(delim),
    }
}

pub(super) fn accent_combining(label: &str) -> Option<char> {
    match label {
        r"\dot" => Some('\u{0307}'),
        r"\ddot" => Some('\u{0308}'),
        r"\dddot" => Some('\u{20DB}'),
        r"\hat" | r"\widehat" => Some('\u{0302}'),
        r"\bar" | r"\overline" => Some('\u{0304}'),
        r"\tilde" | r"\widetilde" => Some('\u{0303}'),
        r"\vec" => Some('\u{20D7}'),
        r"\acute" => Some('\u{0301}'),
        r"\grave" => Some('\u{0300}'),
        r"\breve" => Some('\u{0306}'),
        r"\check" => Some('\u{030C}'),
        r"\mathring" => Some('\u{030A}'),
        _ => None,
    }
}

pub(super) fn to_superscript_str(text: &str) -> Option<String> {
    if text.is_empty() {
        return Some(String::new());
    }
    let mut out = String::with_capacity(text.len() * 2);
    for ch in text.chars() {
        if ch == ' ' {
            out.push(' ');
            continue;
        }
        out.push(superscript_char(ch)?);
    }
    Some(out)
}

pub(super) fn to_subscript_str(text: &str) -> Option<String> {
    if text.is_empty() {
        return Some(String::new());
    }
    let mut out = String::with_capacity(text.len() * 2);
    for ch in text.chars() {
        if ch == ' ' {
            out.push(' ');
            continue;
        }
        out.push(subscript_char(ch)?);
    }
    Some(out)
}

fn superscript_char(ch: char) -> Option<char> {
    match ch {
        '0' => Some('⁰'),
        '1' => Some('¹'),
        '2' => Some('²'),
        '3' => Some('³'),
        '4' => Some('⁴'),
        '5' => Some('⁵'),
        '6' => Some('⁶'),
        '7' => Some('⁷'),
        '8' => Some('⁸'),
        '9' => Some('⁹'),
        '+' => Some('⁺'),
        '-' | '−' => Some('⁻'),
        '=' => Some('⁼'),
        '(' => Some('⁽'),
        ')' => Some('⁾'),
        'a' => Some('ᵃ'),
        'b' => Some('ᵇ'),
        'c' => Some('ᶜ'),
        'd' => Some('ᵈ'),
        'e' => Some('ᵉ'),
        'f' => Some('ᶠ'),
        'g' => Some('ᵍ'),
        'h' => Some('ʰ'),
        'i' => Some('ⁱ'),
        'j' => Some('ʲ'),
        'k' => Some('ᵏ'),
        'l' => Some('ˡ'),
        'm' => Some('ᵐ'),
        'n' => Some('ⁿ'),
        'o' => Some('ᵒ'),
        'p' => Some('ᵖ'),
        'r' => Some('ʳ'),
        's' => Some('ˢ'),
        't' => Some('ᵗ'),
        'u' => Some('ᵘ'),
        'v' => Some('ᵛ'),
        'w' => Some('ʷ'),
        'x' => Some('ˣ'),
        'y' => Some('ʸ'),
        'z' => Some('ᶻ'),
        'A' => Some('ᴬ'),
        'B' => Some('ᴮ'),
        'D' => Some('ᴰ'),
        'E' => Some('ᴱ'),
        'G' => Some('ᴳ'),
        'H' => Some('ᴴ'),
        'I' => Some('ᴵ'),
        'J' => Some('ᴶ'),
        'K' => Some('ᴷ'),
        'L' => Some('ᴸ'),
        'M' => Some('ᴹ'),
        'N' => Some('ᴺ'),
        'O' => Some('ᴼ'),
        'P' => Some('ᴾ'),
        'R' => Some('ᴿ'),
        'T' => Some('ᵀ'),
        'U' => Some('ᵁ'),
        'V' => Some('ⱽ'),
        'W' => Some('ᵂ'),
        'α' => Some('ᵅ'),
        'β' => Some('ᵝ'),
        'γ' => Some('ᵞ'),
        'δ' => Some('ᵟ'),
        'ε' | 'ϵ' => Some('ᵋ'),
        'θ' => Some('ᶿ'),
        'ι' => Some('ᶥ'),
        'φ' | 'ϕ' => Some('ᵠ'),
        'χ' => Some('ᵡ'),
        '′' | '\'' => Some('′'),
        _ => None,
    }
}

fn subscript_char(ch: char) -> Option<char> {
    match ch {
        '0' => Some('₀'),
        '1' => Some('₁'),
        '2' => Some('₂'),
        '3' => Some('₃'),
        '4' => Some('₄'),
        '5' => Some('₅'),
        '6' => Some('₆'),
        '7' => Some('₇'),
        '8' => Some('₈'),
        '9' => Some('₉'),
        '+' => Some('₊'),
        '-' | '−' => Some('₋'),
        '=' => Some('₌'),
        '(' => Some('₍'),
        ')' => Some('₎'),
        'a' => Some('ₐ'),
        'e' => Some('ₑ'),
        'h' => Some('ₕ'),
        'i' => Some('ᵢ'),
        'j' => Some('ⱼ'),
        'k' => Some('ₖ'),
        'l' => Some('ₗ'),
        'm' => Some('ₘ'),
        'n' => Some('ₙ'),
        'o' => Some('ₒ'),
        'p' => Some('ₚ'),
        'r' => Some('ᵣ'),
        's' => Some('ₛ'),
        't' => Some('ₜ'),
        'u' => Some('ᵤ'),
        'v' => Some('ᵥ'),
        'x' => Some('ₓ'),
        'β' => Some('ᵦ'),
        'γ' => Some('ᵧ'),
        'ρ' => Some('ᵨ'),
        'φ' | 'ϕ' => Some('ᵩ'),
        'χ' => Some('ᵪ'),
        _ => None,
    }
}

pub(super) fn apply_math_font(font: &str, text: &str) -> String {
    match font {
        "mathbb" => text
            .chars()
            .map(|ch| match ch {
                'C' => 'ℂ',
                'H' => 'ℍ',
                'N' => 'ℕ',
                'P' => 'ℙ',
                'Q' => 'ℚ',
                'R' => 'ℝ',
                'Z' => 'ℤ',
                'A'..='Z' => char::from_u32(0x1D538 + (ch as u32 - 'A' as u32)).unwrap_or(ch),
                'a'..='z' => char::from_u32(0x1D552 + (ch as u32 - 'a' as u32)).unwrap_or(ch),
                '0'..='9' => char::from_u32(0x1D7D8 + (ch as u32 - '0' as u32)).unwrap_or(ch),
                _ => ch,
            })
            .collect(),
        "mathbf" | "boldsymbol" => text
            .chars()
            .map(|ch| match ch {
                'A'..='Z' => char::from_u32(0x1D400 + (ch as u32 - 'A' as u32)).unwrap_or(ch),
                'a'..='z' => char::from_u32(0x1D41A + (ch as u32 - 'a' as u32)).unwrap_or(ch),
                '0'..='9' => char::from_u32(0x1D7CE + (ch as u32 - '0' as u32)).unwrap_or(ch),
                _ => ch,
            })
            .collect(),
        "mathcal" | "mathscr" => text
            .chars()
            .map(|ch| match ch {
                'B' => 'ℬ',
                'E' => 'ℰ',
                'F' => 'ℱ',
                'H' => 'ℋ',
                'I' => 'ℐ',
                'L' => 'ℒ',
                'M' => 'ℳ',
                'R' => 'ℛ',
                'A'..='Z' => char::from_u32(0x1D49C + (ch as u32 - 'A' as u32)).unwrap_or(ch),
                _ => ch,
            })
            .collect(),
        "mathfrak" => text
            .chars()
            .map(|ch| match ch {
                'C' => 'ℭ',
                'H' => 'ℌ',
                'I' => 'ℑ',
                'R' => 'ℜ',
                'Z' => 'ℨ',
                'A'..='Z' => char::from_u32(0x1D504 + (ch as u32 - 'A' as u32)).unwrap_or(ch),
                'a'..='z' => char::from_u32(0x1D51E + (ch as u32 - 'a' as u32)).unwrap_or(ch),
                _ => ch,
            })
            .collect(),
        _ => text.to_owned(),
    }
}

pub(crate) fn format_html_superscript(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let converted: Option<String> = trimmed
        .chars()
        .map(|ch| match ch {
            '*' | '†' | '‡' | '§' | '¶' | ',' | ' ' => Some(ch),
            _ => superscript_char(ch),
        })
        .collect();
    if let Some(unicode) = converted {
        return unicode;
    }
    if trimmed.chars().any(char::is_whitespace) {
        format!("^({trimmed})")
    } else {
        format!("^{trimmed}")
    }
}

pub(crate) fn format_html_subscript(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let converted: Option<String> = trimmed
        .chars()
        .map(|ch| match ch {
            ',' | ' ' => Some(ch),
            _ => subscript_char(ch),
        })
        .collect();
    if let Some(unicode) = converted {
        return unicode;
    }
    if trimmed.chars().any(char::is_whitespace) {
        format!("_({trimmed})")
    } else {
        format!("_{trimmed}")
    }
}

pub(crate) fn decode_superscript_label(label: &str) -> Option<String> {
    if label.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(label.len());
    let mut converted_any = false;
    for ch in label.chars() {
        out.push(from_superscript_char(ch)?);
        converted_any = true;
    }
    converted_any.then_some(out)
}

fn from_superscript_char(ch: char) -> Option<char> {
    match ch {
        '⁰' => Some('0'),
        '¹' => Some('1'),
        '²' => Some('2'),
        '³' => Some('3'),
        '⁴' => Some('4'),
        '⁵' => Some('5'),
        '⁶' => Some('6'),
        '⁷' => Some('7'),
        '⁸' => Some('8'),
        '⁹' => Some('9'),
        '⁺' => Some('+'),
        '⁻' => Some('-'),
        '⁼' => Some('='),
        '⁽' => Some('('),
        '⁾' => Some(')'),
        'ᵃ' => Some('a'),
        'ᵇ' => Some('b'),
        'ᶜ' => Some('c'),
        'ᵈ' => Some('d'),
        'ᵉ' => Some('e'),
        'ᶠ' => Some('f'),
        'ᵍ' => Some('g'),
        'ʰ' => Some('h'),
        'ⁱ' => Some('i'),
        'ʲ' => Some('j'),
        'ᵏ' => Some('k'),
        'ˡ' => Some('l'),
        'ᵐ' => Some('m'),
        'ⁿ' => Some('n'),
        'ᵒ' => Some('o'),
        'ᵖ' => Some('p'),
        'ʳ' => Some('r'),
        'ˢ' => Some('s'),
        'ᵗ' => Some('t'),
        'ᵘ' => Some('u'),
        'ᵛ' => Some('v'),
        'ʷ' => Some('w'),
        'ˣ' => Some('x'),
        'ʸ' => Some('y'),
        'ᶻ' => Some('z'),
        _ => None,
    }
}
