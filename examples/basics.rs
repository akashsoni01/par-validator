mod iso_pain {
    type IsoError = crate::RuleBuilderError<String>;
    // For raw rule — still receives Option<&String>
    pub fn iso123rule<'a>(r: Option<&'a String>) -> IsoError {
        if r.map_or(true, |s| s.trim().is_empty()) {
            IsoError::Fail("123 rule failed".to_string())
        } else {
            IsoError::Success
        }
    }

    // For mandatory/optional — receives &String directly, None already handled
    pub fn not_blank<'a>(s: &'a String) -> IsoError {
        if s.trim().is_empty() {
            IsoError::Fail("blank field".to_string())
        } else {
            IsoError::Success
        }
    }

    pub fn max_len_35<'a>(s: &'a String) -> IsoError {
        if s.len() > 35 {
            IsoError::Fail("max_len_35".to_string())
        } else {
            IsoError::Success
        }
    }
}

#[derive(Kp)]
struct Test {
    a: String,
    b: String,

}

fn main() {
    let t = Test {
        a: "  ".to_string(),
        b: "asdf ".to_string(),
    };

    let rules = [RuleBuilder::new(Test::a())
        .with_root(&t)
        .rule(iso_pain::iso123rule)
        .rule(iso_pain::iso123rule)
        .rule(iso_pain::iso123rule)
        .rule(iso_pain::iso123rule)
        .madatory_rule(iso_pain::iso123rule),


        RuleBuilder::new(Test::b())
        .with_root(&t)
        .rule(iso_pain::iso123rule)
        .rule(iso_pain::iso123rule)
        .rule(iso_pain::iso123rule)
        .rule(iso_pain::iso123rule)
        .madatory_rule(iso_pain::iso123rule),
        ];
        // let errors = rules
        // .iter()
        // .fold(Vec::new(), |mut acc, v|  {acc.append(&mut v.apply()); acc} );

        let errors: Vec<RuleBuilderError<String>> = rules.iter().flat_map(|v| v.apply()).collect();
        
        for e in errors {
            println!("{:?}", e);
        }
}
