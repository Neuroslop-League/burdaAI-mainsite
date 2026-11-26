use askama::Template;
use serde::Deserialize;

type Text = (&'static str, &'static str);

struct Definition {
    id: &'static str,
    title: Text,
    label: &'static str,
    tone: &'static str,
    description: Text,
    document: &'static str,
}

const COMPONENTS: [Definition; 5] = [
    Definition {
        id: "tokenizer",
        title: ("Токенизатор", "Tokenizer"),
        label: "C02",
        tone: "training",
        description: (
            "Преобразует текст в идентификаторы токенов и обратно. Один и тот же артефакт токенизатора используют при подготовке данных, обучении и локальном запуске.",
            "Converts text to token IDs and back. Data preparation, training and local inference use the same tokenizer artifact.",
        ),
        document: "tokenizer.md",
    },
    Definition {
        id: "data",
        title: ("Данные", "Data"),
        label: "C04",
        tone: "model",
        description: (
            "Подготовка корпуса проверяет записи, делит их на обучающую и оценочные выборки и собирает токенизированные шарды с манифестом.",
            "Corpus preparation validates records, splits them into training and evaluation sets, and produces tokenized shards with a manifest.",
        ),
        document: "data.md",
    },
    Definition {
        id: "model",
        title: ("Модель", "Model"),
        label: "C01 / C03",
        tone: "model",
        description: (
            "Декодер Transformer предсказывает следующий токен по предшествующему контексту. В архитектуре 20 слоёв, групповое внимание GQA и окно 4096 токенов.",
            "The Transformer decoder predicts the next token from preceding context. It uses 20 layers, grouped-query attention and a 4,096-token context window.",
        ),
        document: "model-spec.md",
    },
    Definition {
        id: "training",
        title: ("Обучение", "Training"),
        label: "C05",
        tone: "training",
        description: (
            "PyTorch обновляет веса декодера на подготовленных данных. Контрольная точка сохраняет веса и состояние, необходимое для возобновления обучения.",
            "PyTorch updates the decoder weights using the prepared data. Checkpoints save the weights and the state needed to resume training.",
        ),
        document: "training.md",
    },
    Definition {
        id: "runtime",
        title: ("Rust-рантайм", "Rust runtime"),
        label: "C07",
        tone: "runtime",
        description: (
            "Загружает экспортированный комплект весов и токенизатор и генерирует текст локально. Для запуска предусмотрены Rust CLI и локальный API.",
            "Loads the exported weights and tokenizer and generates text locally. It provides a Rust CLI and a local API.",
        ),
        document: "inference-bundle.md",
    },
];

pub struct Edge {
    pub from: &'static str,
    pub to: &'static str,
}

const EDGES: &[Edge] = &[
    Edge {
        from: "tokenizer",
        to: "data",
    },
    Edge {
        from: "tokenizer",
        to: "training",
    },
    Edge {
        from: "data",
        to: "training",
    },
    Edge {
        from: "model",
        to: "training",
    },
    Edge {
        from: "training",
        to: "runtime",
    },
    Edge {
        from: "tokenizer",
        to: "runtime",
    },
];

#[derive(Deserialize)]
pub struct Selection {
    node: Option<String>,
}

pub struct ComponentLink {
    pub id: &'static str,
    pub title: &'static str,
}

impl ComponentLink {
    fn from_definition(node: &Definition, en: bool) -> Self {
        Self {
            id: node.id,
            title: if en { node.title.1 } else { node.title.0 },
        }
    }
}

pub fn component_links(ids: &[&str], en: bool) -> Vec<ComponentLink> {
    ids.iter()
        .filter_map(|id| COMPONENTS.iter().find(|node| node.id == *id))
        .map(|node| ComponentLink::from_definition(node, en))
        .collect()
}

pub struct Component {
    pub node: ComponentLink,
    pub label: &'static str,
    pub tone: &'static str,
    pub description: &'static str,
    pub document: &'static str,
    pub incoming: Vec<ComponentLink>,
    pub outgoing: Vec<ComponentLink>,
}

#[derive(Template)]
#[template(path = "project_map.html")]
pub struct Map {
    en: bool,
    active: &'static str,
    components: Vec<Component>,
    edges: &'static [Edge],
}

impl Map {
    pub fn new(en: bool, selection: &Selection) -> Self {
        let title = |text: Text| if en { text.1 } else { text.0 };
        let active = COMPONENTS
            .iter()
            .find(|node| Some(node.id) == selection.node.as_deref())
            .map_or("training", |node| node.id);
        let link = |node: &Definition| ComponentLink::from_definition(node, en);
        let components = COMPONENTS
            .iter()
            .map(|node| {
                let incoming = COMPONENTS
                    .iter()
                    .filter(|other| {
                        EDGES
                            .iter()
                            .any(|edge| edge.from == other.id && edge.to == node.id)
                    })
                    .map(link)
                    .collect();
                let outgoing = COMPONENTS
                    .iter()
                    .filter(|other| {
                        EDGES
                            .iter()
                            .any(|edge| edge.from == node.id && edge.to == other.id)
                    })
                    .map(link)
                    .collect();
                Component {
                    node: link(node),
                    label: node.label,
                    tone: node.tone,
                    description: title(node.description),
                    document: node.document,
                    incoming,
                    outgoing,
                }
            })
            .collect();
        Self {
            en,
            active,
            components,
            edges: EDGES,
        }
    }
}
