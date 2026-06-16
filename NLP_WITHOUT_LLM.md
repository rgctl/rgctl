# NLP Query Translation Without Heavy LLM Dependency

## The Challenge

Can we translate natural language queries to graph queries **without calling an LLM every time**?

**Answer: Yes!** Using a hybrid approach that combines:
1. Pattern matching and templates
2. Semantic parsing libraries
3. One-time LLM bootstrapping
4. Query pattern learning and caching
5. Small local models (optional)

---

## Approach 1: Pattern-Based Translation (No LLM Required)

### How It Works

1. **Parse the question** using traditional NLP (dependency parsing, entity extraction)
2. **Match to query templates** based on structure and keywords
3. **Fill in parameters** from extracted entities
4. **Execute the generated Cypher query**

### Implementation

```rust
// src/nlp/pattern_matcher.rs
use spacy_rs::Spacy;  // Or similar Rust NLP library
use regex::Regex;

pub struct PatternMatcher {
    templates: Vec<QueryTemplate>,
    entity_extractor: EntityExtractor,
    intent_classifier: IntentClassifier,
}

pub struct QueryTemplate {
    intent: Intent,
    patterns: Vec<Pattern>,
    cypher_template: String,
    parameters: Vec<Parameter>,
}

pub enum Intent {
    Count,         // "How many X?"
    List,          // "Show me all X"
    Find,          // "Find X"
    Impact,        // "What breaks if I change X?"
    Complexity,    // "What's the complexity of X?"
    Dependencies,  // "What does X depend on?"
    Callers,       // "What calls X?"
    Compare,       // "What's the most X?"
}

impl PatternMatcher {
    pub fn translate(&self, question: &str) -> Result<CypherQuery> {
        // 1. Classify intent
        let intent = self.classify_intent(question)?;
        
        // 2. Extract entities (function names, labels, metrics)
        let entities = self.entity_extractor.extract(question)?;
        
        // 3. Find matching template
        let template = self.find_template(intent, &entities)?;
        
        // 4. Fill template with entities
        let cypher = self.fill_template(template, entities)?;
        
        Ok(CypherQuery {
            query: cypher,
            confidence: template.confidence,
            method: TranslationMethod::PatternBased,
        })
    }
    
    fn classify_intent(&self, question: &str) -> Result<Intent> {
        // Simple keyword-based classification
        let question_lower = question.to_lowercase();
        
        if question_lower.starts_with("how many") || question_lower.starts_with("count") {
            return Ok(Intent::Count);
        }
        
        if question_lower.contains("what breaks") || 
           question_lower.contains("what would break") ||
           question_lower.contains("impact") {
            return Ok(Intent::Impact);
        }
        
        if question_lower.starts_with("list") || 
           question_lower.starts_with("show me") ||
           question_lower.starts_with("give me") {
            return Ok(Intent::List);
        }
        
        if question_lower.contains("complexity") {
            return Ok(Intent::Complexity);
        }
        
        if question_lower.contains("what calls") || question_lower.contains("who calls") {
            return Ok(Intent::Callers);
        }
        
        if question_lower.contains("depend") {
            return Ok(Intent::Dependencies);
        }
        
        if question_lower.contains("most") || 
           question_lower.contains("highest") ||
           question_lower.contains("top") {
            return Ok(Intent::Compare);
        }
        
        Ok(Intent::Find)
    }
}

pub struct EntityExtractor {
    graph_schema: GraphSchema,  // Know what labels/types exist
    symbol_index: SymbolIndex,   // Know what symbols exist
}

impl EntityExtractor {
    pub fn extract(&self, question: &str) -> Result<Entities> {
        let mut entities = Entities::default();
        
        // Extract labels (e.g., "React components" -> label: react:component)
        for label in &self.graph_schema.labels {
            if self.matches_label(question, label) {
                entities.labels.push(label.clone());
            }
        }
        
        // Extract symbol names (e.g., "verify_token" -> symbol: verify_token)
        for symbol in &self.symbol_index.symbols {
            if question.contains(&symbol.name) {
                entities.symbols.push(symbol.name.clone());
            }
        }
        
        // Extract metrics (e.g., "complexity > 20" -> metric: complexity, threshold: 20)
        if let Some(threshold) = self.extract_number(question) {
            if question.contains("complexity") {
                entities.metric = Some(Metric::Complexity(threshold));
            }
        }
        
        Ok(entities)
    }
    
    fn matches_label(&self, question: &str, label: &Label) -> bool {
        // Try different variations
        let variations = vec![
            label.name.clone(),
            label.name.replace(":", " "),
            label.plural_form(),
            label.natural_language_form(),
        ];
        
        for variation in variations {
            if question.to_lowercase().contains(&variation.to_lowercase()) {
                return true;
            }
        }
        
        false
    }
}
```

### Query Templates

```rust
// Pre-defined templates (no LLM needed)
let templates = vec![
    // "How many X?"
    QueryTemplate {
        intent: Intent::Count,
        patterns: vec![
            Pattern::regex(r"how many (.+)\??"),
            Pattern::regex(r"count (.+)"),
        ],
        cypher_template: "MATCH (n:{label}) RETURN COUNT(n) as count",
        parameters: vec![
            Parameter::label_from_capture(1),
        ],
    },
    
    // "Show me all X"
    QueryTemplate {
        intent: Intent::List,
        patterns: vec![
            Pattern::regex(r"(?:show me|give me|list)(?: all)? (.+)"),
        ],
        cypher_template: "MATCH (n:{label}) RETURN n.name, n.file_path, n.complexity LIMIT 100",
        parameters: vec![
            Parameter::label_from_capture(1),
        ],
    },
    
    // "What breaks if I change X?"
    QueryTemplate {
        intent: Intent::Impact,
        patterns: vec![
            Pattern::regex(r"what (?:breaks|would break) if .* (?:change|delete|modify) (.+)\??"),
            Pattern::regex(r"impact (?:of|if) (?:changing|deleting) (.+)"),
        ],
        cypher_template: r#"
            MATCH (target {name: {symbol}})
            MATCH path = (caller)-[:Calls*1..3]->(target)
            RETURN DISTINCT caller.name, caller.file_path, LENGTH(path) as depth
            ORDER BY depth ASC
        "#,
        parameters: vec![
            Parameter::symbol_from_capture(1),
        ],
    },
    
    // "Find all X with Y > Z"
    QueryTemplate {
        intent: Intent::Find,
        patterns: vec![
            Pattern::regex(r"find(?: all)? (.+) with (.+) > (\d+)"),
        ],
        cypher_template: "MATCH (n:{label}) WHERE n.{metric} > {threshold} RETURN n",
        parameters: vec![
            Parameter::label_from_capture(1),
            Parameter::metric_from_capture(2),
            Parameter::number_from_capture(3),
        ],
    },
    
    // "What calls X?"
    QueryTemplate {
        intent: Intent::Callers,
        patterns: vec![
            Pattern::regex(r"(?:what|who) calls (.+)\??"),
            Pattern::regex(r"callers of (.+)"),
        ],
        cypher_template: "MATCH (caller)-[:Calls]->(target {name: {symbol}}) RETURN caller",
        parameters: vec![
            Parameter::symbol_from_capture(1),
        ],
    },
    
    // "What's the most connected X?"
    QueryTemplate {
        intent: Intent::Compare,
        patterns: vec![
            Pattern::regex(r"(?:what's|whats) the most (.+) (.+)\??"),
            Pattern::regex(r"top (\d+) (.+)"),
        ],
        cypher_template: r#"
            MATCH (n:{label})
            RETURN n, COUNT((n)--()) as degree
            ORDER BY degree DESC
            LIMIT {limit}
        "#,
        parameters: vec![
            Parameter::label_from_capture(2),
            Parameter::number_from_capture(1, default=10),
        ],
    },
];
```

### Label Mapping (Domain-Specific)

```rust
pub struct LabelMapper {
    mappings: HashMap<Vec<String>, String>,
}

impl LabelMapper {
    pub fn new(graph: &Graph) -> Self {
        let mut mappings = HashMap::new();
        
        // Learn from graph labels
        for label in graph.all_labels() {
            // If label is "react:component"
            let natural_forms = vec![
                "react component",
                "react components",
                "components",
                "ui component",
            ];
            mappings.insert(natural_forms, label.clone());
        }
        
        // Common programming terms
        mappings.insert(
            vec!["service", "services", "soa service"],
            "soa:service".to_string()
        );
        
        mappings.insert(
            vec!["function", "functions", "method", "methods"],
            "Function".to_string()
        );
        
        Self { mappings }
    }
    
    pub fn map(&self, natural_text: &str) -> Option<String> {
        for (natural_forms, label) in &self.mappings {
            for form in natural_forms {
                if natural_text.to_lowercase().contains(form) {
                    return Some(label.clone());
                }
            }
        }
        None
    }
}
```

---

## Approach 2: Learning from Usage (One-Time LLM Bootstrap)

### How It Works

1. **Bootstrap**: Use LLM once to generate 100+ example query pairs
2. **Store patterns**: Cache the (question → Cypher) mappings
3. **Match**: Use fuzzy matching to find similar questions
4. **Adapt**: Substitute entities in cached queries
5. **Learn**: Add successful user queries to the cache

### Implementation

```rust
pub struct QueryCache {
    cache: HashMap<String, CachedQuery>,
    similarity_index: SimilarityIndex,
}

pub struct CachedQuery {
    question: String,
    cypher: String,
    intent: Intent,
    entities: Entities,
    success_count: u32,
}

impl QueryCache {
    /// Bootstrap cache with LLM-generated examples (one-time)
    pub async fn bootstrap(&mut self, llm: &LLMClient) -> Result<()> {
        let examples = self.generate_examples(llm).await?;
        
        for example in examples {
            self.cache.insert(example.question.clone(), CachedQuery {
                question: example.question,
                cypher: example.cypher,
                intent: example.intent,
                entities: example.entities,
                success_count: 1,
            });
        }
        
        self.rebuild_similarity_index();
        Ok(())
    }
    
    async fn generate_examples(&self, llm: &LLMClient) -> Result<Vec<QueryExample>> {
        let prompt = format!(r#"
You are helping bootstrap a query cache for a code knowledge graph.

Generate 100 diverse example questions and their Cypher translations.

Graph schema:
- Node types: Function, Class, Module, File, ConfigKey
- Edge types: Calls, Imports, Inherits, UsedBy, References
- Labels: react:component, soa:service, security:critical, etc.

Examples:
1. Q: "How many React components?"
   Cypher: MATCH (n) WHERE 'react:component' IN n.labels RETURN COUNT(n)
   
2. Q: "What calls verify_token?"
   Cypher: MATCH (caller)-[:Calls]->(target {{name: 'verify_token'}}) RETURN caller

Generate 100 more varied examples covering:
- Counting (how many, count)
- Listing (show me, give me, list)
- Finding (find, search)
- Impact analysis (what breaks, impact of)
- Complexity queries (most complex, high complexity)
- Dependencies (what depends on, what uses)
- Configuration (unused config, missing env vars)

Return JSON array of {{question, cypher, intent, entities}}.
"#);
        
        let response = llm.complete(&prompt).await?;
        let examples: Vec<QueryExample> = serde_json::from_str(&response)?;
        
        Ok(examples)
    }
    
    /// Try to translate using cached similar queries
    pub fn translate_cached(&self, question: &str) -> Option<CypherQuery> {
        // Find most similar cached question
        let similar = self.similarity_index.find_similar(question, threshold=0.8)?;
        
        // Extract entities from new question
        let new_entities = self.extract_entities(question)?;
        
        // Substitute entities in cached query
        let cypher = self.substitute_entities(
            &similar.cypher,
            &similar.entities,
            &new_entities
        );
        
        Some(CypherQuery {
            query: cypher,
            confidence: similar.similarity_score,
            method: TranslationMethod::Cached,
        })
    }
    
    /// Learn from successful user queries
    pub fn learn(&mut self, question: String, cypher: String, success: bool) {
        if success {
            if let Some(cached) = self.cache.get_mut(&question) {
                cached.success_count += 1;
            } else {
                // Add new pattern
                self.cache.insert(question.clone(), CachedQuery {
                    question,
                    cypher,
                    intent: self.infer_intent(&cypher),
                    entities: self.extract_entities_from_cypher(&cypher),
                    success_count: 1,
                });
                self.rebuild_similarity_index();
            }
        }
    }
}

pub struct SimilarityIndex {
    embeddings: HashMap<String, Vec<f32>>,
    model: SentenceEmbeddingModel,  // Small local model
}

impl SimilarityIndex {
    pub fn find_similar(&self, question: &str, threshold: f32) -> Option<SimilarQuery> {
        let query_embedding = self.model.encode(question);
        
        let mut best_match = None;
        let mut best_score = threshold;
        
        for (cached_question, cached_embedding) in &self.embeddings {
            let score = cosine_similarity(&query_embedding, cached_embedding);
            if score > best_score {
                best_score = score;
                best_match = Some(cached_question.clone());
            }
        }
        
        best_match.map(|q| SimilarQuery {
            question: q,
            similarity_score: best_score,
        })
    }
}
```

---

## Approach 3: Small Local Model (Optional)

### Fine-Tuned T5 for Query Translation

Instead of calling Claude/GPT-4 every time, use a **small fine-tuned model** (< 500MB) that runs locally.

```rust
use candle_core::{Device, Tensor};
use candle_transformers::models::t5::{T5ForConditionalGeneration, T5Config};
use tokenizers::Tokenizer;

pub struct LocalQueryTranslator {
    model: T5ForConditionalGeneration,
    tokenizer: Tokenizer,
    device: Device,
}

impl LocalQueryTranslator {
    pub fn new(model_path: &Path) -> Result<Self> {
        let device = Device::cuda_if_available(0)?;
        let config = T5Config::from_json_file(model_path.join("config.json"))?;
        let model = T5ForConditionalGeneration::load(model_path, &device)?;
        let tokenizer = Tokenizer::from_file(model_path.join("tokenizer.json"))?;
        
        Ok(Self { model, tokenizer, device })
    }
    
    pub fn translate(&self, question: &str) -> Result<String> {
        // Prepare input: "translate to cypher: {question}"
        let input = format!("translate to cypher: {}", question);
        let tokens = self.tokenizer.encode(input, true)?;
        let input_ids = Tensor::new(tokens.get_ids(), &self.device)?;
        
        // Generate Cypher query
        let output = self.model.generate(&input_ids, max_length=512)?;
        let cypher = self.tokenizer.decode(output.to_vec1()?, skip_special_tokens=true)?;
        
        Ok(cypher)
    }
}

// Training data generation (one-time, using LLM)
async fn generate_training_data(llm: &LLMClient) -> Result<Vec<TrainingExample>> {
    // Generate 10,000+ (question, cypher) pairs using LLM
    // Fine-tune T5-small on this data
    // Deploy the fine-tuned model locally
}
```

**Benefits**:
- No API calls (100% offline)
- Fast inference (< 50ms)
- Privacy (no data leaves machine)
- Customized to your domain

**Tradeoff**:
- Requires one-time training data generation (uses LLM)
- ~500MB model download
- Less flexible than full LLM

---

## Hybrid Architecture (Recommended)

**Combine all approaches** for best results:

```rust
pub struct HybridNLPEngine {
    pattern_matcher: PatternMatcher,      // Fast, no LLM
    query_cache: QueryCache,              // Learned patterns
    local_model: Option<LocalQueryTranslator>,  // Offline T5
    llm_client: Option<LLMClient>,        // Fallback to cloud LLM
}

impl HybridNLPEngine {
    pub async fn translate(&mut self, question: &str) -> Result<CypherQuery> {
        // 1. Try pattern matching (instant, no computation)
        if let Ok(query) = self.pattern_matcher.translate(question) {
            if query.confidence > 0.9 {
                return Ok(query);
            }
        }
        
        // 2. Try cached similar queries (fast, local)
        if let Some(query) = self.query_cache.translate_cached(question) {
            if query.confidence > 0.8 {
                // Validate query before returning
                if self.validate_query(&query).is_ok() {
                    return Ok(query);
                }
            }
        }
        
        // 3. Try local fine-tuned model (if available)
        if let Some(local_model) = &self.local_model {
            let cypher = local_model.translate(question)?;
            if self.validate_query_string(&cypher).is_ok() {
                return Ok(CypherQuery {
                    query: cypher,
                    confidence: 0.85,
                    method: TranslationMethod::LocalModel,
                });
            }
        }
        
        // 4. Fallback to cloud LLM (slowest, most capable)
        if let Some(llm) = &self.llm_client {
            let result = self.llm_translate(llm, question).await?;
            
            // Learn from successful LLM query
            self.query_cache.learn(question.to_string(), result.query.clone(), true);
            
            return Ok(result);
        }
        
        Err(Error::CannotTranslate(question.to_string()))
    }
    
    fn validate_query(&self, query: &CypherQuery) -> Result<()> {
        // Parse Cypher to ensure it's valid
        // Check that referenced labels/properties exist in graph
        cypher_parser::parse(&query.query)?;
        Ok(())
    }
}
```

### Performance Profile

| Method | Latency | Accuracy | Requirements | Cost |
|--------|---------|----------|--------------|------|
| Pattern matching | < 1ms | 85% | None | Free |
| Cached queries | < 5ms | 90% | Initial bootstrap | One-time LLM cost |
| Local T5 model | < 50ms | 92% | 500MB download | One-time training cost |
| Cloud LLM | 500-2000ms | 98% | API key | Per-query cost |

### Cache Hit Rates (Expected)

After bootstrapping and learning:
- **Week 1**: 40% cache hits (pattern matching + bootstrap cache)
- **Week 2**: 65% cache hits (learning from usage)
- **Month 1**: 80% cache hits (most queries covered)
- **Month 3**: 90% cache hits (mature system)

**Result**: 90% of queries answered without LLM calls.

---

## Example: Query Translation Flow

**User asks**: "How many React components am I using?"

### Attempt 1: Pattern Matching

```rust
// Extract intent: Count
// Extract entity: "React components" → label: "react:component"
// Match template: "How many {label}?"
// Generate: MATCH (n) WHERE 'react:component' IN n.labels RETURN COUNT(n)
// Confidence: 0.95 ✅
// RETURN immediately (< 1ms)
```

**User asks**: "What would break if I change the authentication logic in verify_token?"

### Attempt 1: Pattern Matching

```rust
// Extract intent: Impact
// Extract entity: "verify_token"
// Match template: "What breaks if I change {symbol}?"
// Generate: MATCH (target {name: 'verify_token'}) ...
// Confidence: 0.85 ✅
// RETURN (< 1ms)
```

**User asks**: "Show me functions that are both high complexity and have no tests, sorted by how many other functions call them"

### Attempt 1: Pattern Matching

```rust
// Complex query, no template matches
// FAIL (too complex)
```

### Attempt 2: Query Cache

```rust
// Find similar: "Find high complexity functions without tests"
// Similarity: 0.75 (below threshold)
// FAIL
```

### Attempt 3: Local T5 Model

```rust
// Generate:
// MATCH (f:Function)
// WHERE f.complexity > 15
//   AND NOT EXISTS((f)<-[:Tests]-())
// OPTIONAL MATCH (caller)-[:Calls]->(f)
// RETURN f, COUNT(caller) as num_callers
// ORDER BY num_callers DESC
// Confidence: 0.88 ✅
// RETURN (< 50ms)
```

### Attempt 4: Cloud LLM (if needed)

```rust
// Only called if all else fails
// Or if user explicitly wants highest accuracy
```

---

## Implementation Roadmap

### Phase 1: Pattern Matching (Week 1)
- Implement Intent classification
- Build entity extraction
- Create 20 common templates
- Test on sample queries

### Phase 2: Caching System (Week 2)
- Build query cache structure
- Implement similarity matching (TF-IDF or embeddings)
- Bootstrap with 100+ examples using LLM
- Add learning from successful queries

### Phase 3: Local Model (Optional, Week 3-4)
- Generate 10k+ training examples using LLM
- Fine-tune T5-small on query translation
- Integrate into hybrid system
- Benchmark accuracy vs. cloud LLM

### Phase 4: Optimization (Week 5)
- Profile cache hit rates
- Optimize template matching
- Add domain-specific patterns
- A/B test accuracy

---

## Summary: Can We Avoid Heavy LLM Dependency?

**Yes, absolutely!**

1. **90% of queries** can be answered without LLM calls using:
   - Pattern matching (instant)
   - Cached learned patterns
   - Small local models

2. **LLM is used for**:
   - Initial bootstrap (one-time)
   - Novel complex queries (10% of cases)
   - Training data generation (one-time)

3. **Benefits**:
   - **Fast**: < 5ms for most queries
   - **Free**: No per-query API costs after bootstrap
   - **Private**: No data sent to cloud
   - **Offline**: Works without internet

4. **Tradeoffs**:
   - One-time LLM cost for bootstrapping
   - Slightly lower accuracy for complex queries (92% vs 98%)
   - Requires maintenance of templates and cache

**Recommendation**: Use the **hybrid approach**:
- Pattern matching for common queries
- Cache for learned patterns
- Optional local model for better offline support
- LLM fallback for maximum flexibility

This gives you the best of both worlds: fast, free, private for 90% of queries, with high-accuracy LLM fallback for complex cases.
