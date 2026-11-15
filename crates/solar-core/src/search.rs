//! La recherche de SOLAR : sans casse ni accents, mot par mot dans n'importe
//! quel ordre, et tolérante à une faute de frappe par mot.

/// Ce qui est tapé, prêt à comparer.
pub struct Query {
    words: Vec<String>,
}

impl Query {
    pub fn new(text: &str) -> Self {
        Self {
            words: normalize(text)
                .split_whitespace()
                .map(str::to_string)
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Le texte contient-il chaque mot cherché ? Une recherche vide trouve tout.
    pub fn matches(&self, text: &str) -> bool {
        self.is_empty() || self.matches_normalized(&normalize(text))
    }

    /// Comme `matches`, pour un texte déjà passé par `normalize` (les longues
    /// conversations le sont une fois pour toutes).
    pub fn matches_normalized(&self, text: &str) -> bool {
        self.words
            .iter()
            .all(|word| text.contains(word.as_str()) || near(word, text))
    }
}

/// Minuscules, sans accents ni apostrophe typographique.
// shortcut: seuls les accents latins courants sont retirés ; passer par une
// décomposition Unicode si d'autres écritures arrivent.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => out.push('a'),
            'ç' => out.push('c'),
            'è' | 'é' | 'ê' | 'ë' => out.push('e'),
            'ì' | 'í' | 'î' | 'ï' => out.push('i'),
            'ñ' => out.push('n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => out.push('o'),
            'ù' | 'ú' | 'û' | 'ü' => out.push('u'),
            'ý' | 'ÿ' => out.push('y'),
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            '’' | '‘' => out.push('\''),
            c => out.push(c),
        }
    }
    out
}

/// Un mot du texte ressemble-t-il au mot cherché, à une faute près ? Le mot
/// tapé peut être le début d'un mot plus long. Les mots courts doivent être
/// exacts : une faute y changerait le sens.
fn near(word: &str, text: &str) -> bool {
    let word: Vec<char> = word.chars().collect();
    if word.len() < 4 {
        return false;
    }
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|candidate| !candidate.is_empty())
        .any(|candidate| {
            let candidate: Vec<char> = candidate.chars().collect();
            // Le mot entier, ou son début à la longueur tapée (une lettre de
            // plus ou de moins, pour une lettre oubliée ou ajoutée).
            (word.len() - 1..=word.len() + 1)
                .filter(|&len| len <= candidate.len())
                .any(|len| distance(&word, &candidate[..len]) <= 1)
        })
}

/// Distance d'édition (insertion, suppression, remplacement, inversion de deux
/// lettres voisines).
fn distance(a: &[char], b: &[char]) -> usize {
    let mut rows = vec![(0..=b.len()).collect::<Vec<_>>(); a.len() + 1];
    for (i, row) in rows.iter_mut().enumerate() {
        row[0] = i;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (rows[i - 1][j] + 1)
                .min(rows[i][j - 1] + 1)
                .min(rows[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = best;
        }
    }
    rows[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finds(query: &str, text: &str) -> bool {
        Query::new(query).matches(text)
    }

    #[test]
    fn case_and_accents_do_not_matter() {
        assert!(finds("methode", "La Méthode"));
        assert!(finds("MÉTHODE", "la methode"));
        assert!(finds("oeuvre", "Œuvre complète"));
        assert!(finds("l'accueil", "l’accueil"));
    }

    #[test]
    fn words_match_in_any_order() {
        assert!(finds("email connexion", "Connexion par e-mail"));
        assert!(!finds("connexion facture", "Connexion par e-mail"));
        assert!(finds("", "tout"));
    }

    #[test]
    fn one_typo_is_forgiven_in_longer_words_even_while_typing() {
        assert!(finds("conexion", "Connexion par e-mail"), "lettre oubliée");
        assert!(
            finds("connexoin", "Connexion par e-mail"),
            "lettres inversées"
        );
        assert!(finds("authentifcation", "Refonte de l'authentification"));
        assert!(finds("conexi", "Connexion"), "début de mot, avec une faute");
        assert!(!finds("cannexoin", "Connexion"), "deux fautes, c'est trop");
        assert!(finds("lgin", "login.rs"), "dès quatre lettres");
        assert!(!finds("apo", "app.rs"), "mot court : exact seulement");
        assert!(finds("login", "src/auth/login.rs"));
    }
}
