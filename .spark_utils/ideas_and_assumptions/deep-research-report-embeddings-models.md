# Modèles d’embedding open source comparables ou supérieurs à Xenova/all-MiniLM-L6-v2 pour exécution locale

## Résumé exécutif

Le modèle **Xenova/all-MiniLM-L6-v2** est une conversion (Transformers.js / ONNX) du très populaire **sentence-transformers/all-MiniLM-L6-v2**, un encodeur de phrases (SBERT) basé sur **MiniLM** (6 couches, *hidden size* 384) conçu pour produire des embeddings **384D** efficaces et rapides. citeturn56search2turn56search3turn6view0  
Si votre objectif est d’obtenir des embeddings **meilleurs** (qualité sémantique et/ou retrieval) tout en restant **téléchargeable et exécutable localement**, les choix les plus robustes aujourd’hui se répartissent en trois “paliers” (tous disponibles sur Hugging Face et exécutables offline) :

- **Upgrade “même classe de poids / ultra-compact”** : **avsolatorio/GIST-all-MiniLM-L6-v2** (≈22,7M params), très proche du gabarit MiniLM-L6 (donc rapide), mais souvent choisi comme alternative moderne orientée retrieval/robustesse. citeturn38view3  
- **Petits modèles retrieval très compétitifs** (≈33,4M params, 384D) : **intfloat/e5-small-v2** et **BAAI/bge-small-en-v1.5** (excellent ratio qualité/latence, très adaptés au CPU + quantization). citeturn50view0turn38view2  
- **Modèles base plus lourds mais plus performants** : **Alibaba-NLP/gte-base-en-v1.5** (≈136,8M params, 768D, contexte jusqu’à 8192 tokens) et **jinaai/jina-embeddings-v2-base-en** (≈137,7M params, MTEB élevé). citeturn54view0turn2view4  

Côté exécution locale, le meilleur “accélérateur universel” (CPU ou GPU) reste le triptyque **PyTorch / sentence-transformers** (simplicité), puis **export ONNX + ONNX Runtime + quantization** (latence & RAM), et enfin **Transformers.js** lorsque vous visez Node / navigateur (avec modèles Xenova ou exports compatibles). citeturn56search4turn56search1turn56search2  

## Modèle de référence

### Ce que vous avez aujourd’hui avec Xenova/all-MiniLM-L6-v2

- **Runtime** : le model card Xenova fournit un exemple direct via **@huggingface/transformers (Transformers.js)** avec un pipeline `feature-extraction`, *pooling mean* et normalisation, et renvoie une sortie de forme **[N, 384]**. citeturn56search2  
- **Référence “source”** : **sentence-transformers/all-MiniLM-L6-v2** est un encodeur de phrases destiné aux phrases / courts paragraphes. citeturn56search3  
- **Ordre de grandeur taille** : MiniLM **L6-H384** est documenté à **≈22M paramètres** (famille MiniLM), ce qui explique sa très bonne vitesse et son faible encombrement. citeturn6view0  
- **Qualité (repère MTEB)** : sur le leaderboard MTEB, **sentence-transformers/all-MiniLM-L6-v2** est donné avec un **score moyen ~56,97** (selon l’extrait du leaderboard). citeturn0search5  

### Ce que “comparable” vs “supérieur” veut dire en pratique

Pour des embeddings “génériques”, les gains se mesurent typiquement via :
- **STS** (Semantic Textual Similarity) : corrélations (Spearman/Pearson) sur des jeux STS (ex. STSBenchmark, STS12–STS17).  
- **MTEB** : agrégation multi-tâches (classification, clustering, retrieval, reranking, etc.). MTEB est explicitement structuré comme un benchmark massif d’embeddings. citeturn41view1  
- **BEIR** (retrieval) : très pertinent si votre usage est la recherche sémantique / RAG ; la famille **E5** insiste sur des résultats BEIR/MTEB solides y compris en zéro-shot. citeturn55search3  

## Familles d’architectures d’embeddings et implications locales

### MiniLM et dérivés compacts

**MiniLM** est une famille de Transformers compressés (distillation) conçue pour garder une bonne qualité à faible coût. Le dépôt Microsoft UniLM documente explicitement des configurations et tailles, notamment **MiniLMv1-L6-H384 ≈22M paramètres**. citeturn6view0  
Implication locale : excellent sur CPU, et très bon candidat pour **ONNX + quantization**, car la structure encodeur (BERT-like) se prête bien à l’optimisation.

### SBERT (Sentence-BERT) : encodeurs bi-encodeurs pour similarité et retrieval

**Sentence-BERT (SBERT)** modifie BERT en **architectures siamese / triplet** pour produire des embeddings comparables via cosine similarity, et vise explicitement à rendre la recherche sémantique efficace (au lieu de cross-encoder coûteux). citeturn55search0  
Implication : la plupart des modèles “sentence-transformers/…” utilisent ce paradigme (bi-encodeur + pooling), très simple à exécuter localement via `sentence-transformers`. citeturn56search4turn56search7  

### MPNet : meilleure pré-formation (souvent meilleure qualité, coût “base”)

**MPNet** propose une pré-formation “masked + permuted” combinant des avantages de BERT et XLNet. citeturn55search1turn55search13  
Un modèle MPNet “base” est typiquement dans l’ordre de **0,1B paramètres** (classe BERT-base). citeturn7view0  
Implication : meilleure qualité potentielle que MiniLM, mais plus lent / plus lourd sur CPU.

### LaBSE : embeddings multilingues/cross-lingual (utile pour francophones)

**LaBSE** (Language‑agnostic BERT Sentence Embedding) vise des embeddings de phrases **multilingues** et cross-lingual, avec une couverture **~109 langues** largement citée par Google et par l’article. citeturn55search2turn55search10turn55search14  
Implication : bon choix si vous indexez du français + d’autres langues (ou si vous faites du cross-lingual retrieval), au prix d’un modèle plus lourd qu’un MiniLM-L6.

### E5 : embeddings contrastifs faibles-supervisés, forts en BEIR/MTEB

Le papier **E5** présente une famille d’embeddings entraînés en contraste avec supervision faible et évalue explicitement sur **BEIR + MTEB** (56 datasets mentionnés), avec une revendication forte de performance en zéro-shot. citeturn55search3turn55search7  
Implication : très bon choix pour retrieval/RAG local, surtout en versions small/base/large selon votre budget machine.

## Comparatif de modèles recommandés disponibles sur Hugging Face

### Tableau comparatif

Les tailles “disque” ci-dessous sont des **ordres de grandeur théoriques** pour les *poids* seuls (hors tokenizer/config), basés sur `#params × taille_dtype` : FP16 ≈ 2 octets/param, INT8 ≈ 1 octet/param, INT4 ≈ 0,5 octet/param. Les champs “latence” sont donnés comme **tendances** (fortement dépendantes de la longueur de séquence et du batch).

| Modèle (HF) | Famille / architecture | Params (≈) | Embedding dim | Contexte max | Taille poids FP16 (≈) | Qualité benchmark (extraits) | Vitesse relative CPU/GPU | ONNX / quantization | Licence |
|---|---|---:|---:|---:|---:|---|---|---|---|
| sentence-transformers/all-MiniLM-L6-v2 | SBERT sur MiniLM L6-H384 | ~22M citeturn6view0turn56search3 | 384 citeturn56search2 | non spécifié | ~42 MB | MTEB avg ~56,97 citeturn0search5 | Très rapide (référence) | Exportable ONNX (Optimum) citeturn56search1turn56search5 | Apache-2.0 citeturn3view0 |
| avsolatorio/GIST-all-MiniLM-L6-v2 | SBERT/MiniLM (gabarit proche) | 22,7M citeturn38view3 | 384 citeturn38view3 | 512 citeturn38view3 | ~43 MB | STS15 Spearman (main_score) ~0,870 citeturn26search1 | Très rapide | Compatible ST, exportable ONNX citeturn38view3turn56search5 | MIT citeturn38view3 |
| intfloat/e5-small-v2 | E5 small (contrastif) | 33,4M citeturn50view0 | 384 citeturn50view0 | 512 citeturn50view0 | ~63,6 MB | (MTEB/BEIR : non spécifié ici ; voir papier E5) citeturn55search3 | Très rapide à rapide | ONNX + quantization recommandé citeturn56search5turn56search17 | MIT citeturn50view0 |
| BAAI/bge-small-en-v1.5 | BGE small | 33,4M citeturn38view2 | 384 citeturn38view2 | 512 citeturn38view2 | ~63,6 MB | non spécifié | Très rapide à rapide | ONNX + quantization recommandé citeturn56search5turn56search17 | MIT citeturn38view2 |
| Alibaba-NLP/gte-base-en-v1.5 | GTE base | 136,8M citeturn54view0 | 768 citeturn54view0 | 8192 citeturn54view0 | ~261 MB | non spécifié | Moyen à lent (surtout si séquences longues) | ONNX export + optimisations possibles citeturn56search1turn56search13 | Apache-2.0 citeturn54view0 |
| jinaai/jina-embeddings-v2-base-en | Jina embeddings v2 base | 137,7M params (card) citeturn2view4 | non spécifié | non spécifié | ~263 MB | MTEB ~68,33 (card) citeturn2view4 | Moyen (plus lourd que MiniLM) | Exportable ONNX citeturn56search1turn56search5 | Apache-2.0 citeturn2view4 |
| jinaai/jina-embedding-b-en-v1 | Jina embeddings v1 (base) | 109,6M citeturn38view0 | 768 citeturn38view0 | 512 citeturn38view0 | ~209 MB | non spécifié | Moyen | ONNX export + quantization citeturn56search1turn56search17 | Apache-2.0 citeturn38view0 |

### Visuel taille vs qualité

Ce graphe illustre **la tendance générale** : plus de paramètres → potentiel de qualité plus élevé, mais coût local plus grand. Ici, seuls les points où un score MTEB explicite est disponible dans les sources capturées sont tracés. citeturn0search5turn2view4turn6view0  

![Taille vs qualité (extraits MTEB)](sandbox:/mnt/data/size_vs_quality_scatter.png)

## Exécution locale, compatibilité bibliothèques et workflows de quantization

### Exécution locale “simple et standard” (Python)

Le chemin le plus direct est **sentence-transformers**, conçu pour charger des modèles d’embeddings et produire des vecteurs en une ligne. La doc SBERT insiste sur l’usage “go-to” et la prise en charge d’embeddings/rerankers. citeturn56search4turn56search0  

Exemple (CPU/GPU via PyTorch installé) :

```bash
pip install -U sentence-transformers
```

```python
from sentence_transformers import SentenceTransformer

model = SentenceTransformer("intfloat/e5-small-v2")  # ou BAAI/bge-small-en-v1.5, etc.
emb = model.encode(
    ["Bonjour le monde", "Salut !"],
    normalize_embeddings=True,
    batch_size=32,
    show_progress_bar=True,
)
print(emb.shape)  # (2, dim)
```

Si vous utilisez `transformers` “pur”, l’idée est la même : tokenizer → modèle → **pooling** (souvent mean pooling) pour passer d’une séquence variable à un vecteur fixe. Le composant “Pooling” est explicitement documenté côté Sentence Transformers (modes cls/mean/max/lasttoken…). citeturn56search7  

### Optimiser pour le CPU local : ONNX Runtime + Optimum

Pour accélérer (latence) et réduire la RAM, un workflow fréquent est : **export ONNX** puis **inférence ONNX Runtime**. La doc Optimum décrit l’export vers ONNX et ce qu’est ONNX comme format interopérable. citeturn56search1  
Le dépôt `optimum-onnx` indique aussi qu’on peut exporter des modèles Transformers / Sentence Transformers et appliquer **optimisation + quantization**. citeturn56search5  

Exemple (export) :

```bash
pip install -U "optimum[onnxruntime]" onnxruntime
optimum-cli export onnx --model intfloat/e5-small-v2 onnx_e5_small/
```

Ensuite, vous chargez côté Optimum avec les classes ORT (principe “AutoModel → ORTModel…”). citeturn56search13  

Quantization : Optimum documente/centralise la logique ONNX Runtime, et la quantization est explicitement présentée comme un levier pour réduire la précision (donc taille) et améliorer les performances. citeturn56search17turn56search5  

### Exécution locale JavaScript (Xenova / Transformers.js)

Le model card **Xenova/all-MiniLM-L6-v2** fournit un exemple complet : installation `npm i @huggingface/transformers` puis `pipeline('feature-extraction', …)` avec `pooling: 'mean'` et `normalize: true`. citeturn56search2  

Exemple :

```bash
npm i @huggingface/transformers
```

```js
import { pipeline } from "@huggingface/transformers";

const extractor = await pipeline("feature-extraction", "Xenova/all-MiniLM-L6-v2");
const output = await extractor(
  ["Bonjour le monde", "Salut !"],
  { pooling: "mean", normalize: true }
);
console.log(output.dims); // typiquement [2, 384]
```

### Diagramme de workflow local (download → quantize → run)

```mermaid
flowchart LR
  A[Choisir un modèle HF<br/>MiniLM / E5 / BGE / GTE / LaBSE] --> B[Télécharger localement<br/>cache HF ou snapshot]
  B --> C[Exécution directe<br/>sentence-transformers / transformers]
  B --> D[Exporter en ONNX (Optimum)]
  D --> E[Optimiser / Quantifier<br/>INT8 (ONNX Runtime)]
  E --> F[Déployer localement<br/>API, batch embeddings, vector DB]
  C --> F
```

## Recommandations pratiques par compromis et cas d’usage

### Meilleur compromis qualité / légèreté (local, généraliste)

Si vous aimez le format MiniLM-L6 (384D, très rapide) mais voulez une alternative moderne dans le même gabarit, **avsolatorio/GIST-all-MiniLM-L6-v2** est un candidat naturel (≈22,7M params, MIT) et reste dans la “classe” ultra-compacte. citeturn38view3turn26search1  
En parallèle, pour un usage retrieval/RAG, **intfloat/e5-small-v2** est extrêmement populaire en local (≈33,4M params, MIT) et la famille E5 est explicitement évaluée sur BEIR/MTEB avec une revendication de très bonnes performances. citeturn50view0turn55search3turn55search7  

### Options ultra-légères (CPU-first)

Rester en **~22M paramètres** (MiniLM L6-H384) est souvent optimal sur CPU. MiniLMv1 L6-H384 est donné à ~22M paramètres. citeturn6view0  
Dans cette gamme, la priorité est généralement :
- embeddings 384D → index vectoriel plus petit,
- export ONNX + INT8 pour latence/RAM (si vous avez un pipeline d’industrialisation). citeturn56search5turn56search17  

### Options “qualité maximale raisonnable” (local mais plus lourd)

- **jinaai/jina-embeddings-v2-base-en** : le model card affiche **MTEB ~68,33** (nettement au-dessus du repère MiniLM-L6-v2 indiqué à ~56,97 dans l’extrait leaderboard). citeturn2view4turn0search5  
- **Alibaba-NLP/gte-base-en-v1.5** : intéressant si vous avez besoin de **contexte long** (max_tokens 8192 côté meta), mais attention : des séquences longues augmentent fortement le coût mémoire/temps des encodeurs à attention quadratique. citeturn54view0  

### Pour un contexte francophone / multilingue

Si votre corpus est majoritairement **français** (ou multilingue), un modèle explicitement multilingue/cross-lingual peut être préférable :  
- **LaBSE** est décrit comme un modèle d’embeddings de phrases multilingue “language‑agnostic” avec ~109 langues, et est référencé par Google et par l’article. citeturn55search2turn55search10turn55search14  

### Licences : privilégier permissif (industrialisation)

Dans la sélection ci-dessus, vous avez principalement :
- **MIT** (ex. bge-small-en-v1.5, e5-small-v2, GIST-all-MiniLM-L6-v2) citeturn38view2turn50view0turn38view3  
- **Apache-2.0** (ex. gte-base-en-v1.5, jina-embeddings-v2-base-en, jina-embedding-b-en-v1, all-MiniLM-L6-v2) citeturn54view0turn2view4turn38view0turn3view0  

Ces licences sont généralement considérées comme **permissives** pour un usage commercial, contrairement à des licences “NC” (non-commercial) qu’on rencontre parfois sur certains modèles d’embeddings.

### Mini check-list “local-ready” (pragmatique)

- Si vous voulez **le plus simple** : `sentence-transformers` (chargement direct + `encode`). citeturn56search4turn56search0  
- Si vous voulez **CPU optimisé** : export ONNX (Optimum) + ONNX Runtime + quantization INT8. citeturn56search1turn56search13turn56search17  
- Si vous voulez **JavaScript offline** : Transformers.js + modèles Xenova (quand disponibles). citeturn56search2  

Enfin, gardez en tête qu’un embedding **384D vs 768D** impacte directement votre stockage vectoriel (taille des vecteurs) et parfois la latence côté index, donc un modèle “plus gros” n’est pas automatiquement meilleur pour un système complet : il faut arbitrer qualité **vs** coût local et coût d’indexation.