# Transaction Queue API - Take-Home Assignment

## Overview

This is a take-home assignment for a Senior Rust Engineer position focused on building high-performance APIs. The task is to implement a rate-limited transaction queue API that can handle high concurrency while maintaining reliability and performance.

## User Story & Business Context

**Scenario**: You're building the backend for a Solana program that handles account creation for various DeFi protocols and applications. While Solana itself handles transaction prioritization, your program has a critical constraint: it can only safely process **20 account creations per block** (400ms intervals) due to program-specific limitations.

**The Problem**:
- DeFi protocols, wallets, and dApps submit account creation requests via API
- Demand often exceeds the 20 accounts/block capacity (3,000 accounts/minute theoretical max)
- Different clients have different SLAs based on their service tier and business requirements
- Fair queuing is essential to prevent high-volume clients from monopolizing the limited capacity

**The Constraint**:
```
Solana Block Time: 400ms
Program Capacity: 20 account creations per block
Theoretical Max: 3,000 accounts/minute
Reality: Need fair queuing + rate limiting for SLA management
```

**What You're Building**:
```
Account Creation Request → Rate Check → Queue Position → Solana Program Call
     ↓                     ↓            ↓               ↓
"Create PDA for user"    "100/min SLA"  "Position #7"   "Next block slot"
```

**Real-World Examples**:
- **Enterprise DeFi Protocol**: "Create 50 user PDAs for new feature launch" (Enterprise Tier: 500/min, priority: high)
- **Premium Wallet Provider**: "Create account for new user onboarding" (Premium Tier: 100/min, priority: medium)
- **Basic Gaming dApp**: "Create player inventory accounts" (Basic Tier: 20/min, priority: low)
- **Enterprise Exchange**: "Bulk create accounts for institutional users" (Enterprise Tier: 500/min, priority: critical)

**Why This Matters**: Your queue API must efficiently allocate the scarce resource (20 accounts/block) across multiple clients while respecting their SLAs and preventing any single client from starving others.

**Technical Context**: "Transactions" in this exercise refers to Solana account creation requests - each containing account metadata, program parameters, and SLA requirements. This is a systems design exercise focusing on resource allocation, rate limiting, and SLA management rather than the actual Solana program logic.

## Task Requirements

### Core Functionality

Implement the transaction submission endpoint (`POST /v1/transactions/submit`) with the following features:

**Example API Call**:
```json
POST /v1/transactions/submit
{
  "account_id": "defi_protocol_premium_001",
  "transaction_data": {
    "account_type": "user_pda",
    "owner_pubkey": "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU",
    "seed": "user_vault_12345",
    "program_id": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
    "space_bytes": 165,
    "lamports": 2039280
  },
  "priority": 7
}
```

**Example Response**:
```json
{
  "transaction_id": "550e8400-e29b-41d4-a716-446655440000",
  "queue_position": 23,
  "estimated_processing_time_seconds": 69,
  "status": "pending"
}
```

1. **Rate Limiting** (per account tier)
   - **Basic Tier**: 20 account creations per minute
   - **Premium Tier**: 100 account creations per minute
   - **Enterprise Tier**: 500 account creations per minute
   - Configurable limits per account in the database
   - Return appropriate 429 status when limit exceeded
   - Include rate limit headers in response

2. **Queue Management**
   - FIFO queue with priority support
   - Persist transactions to PostgreSQL
   - Use Redis for efficient operations
   - Return queue position and estimated time

3. **Performance Requirements**
   - Handle 10,000+ concurrent requests
   - Sub-100ms response time at p99
   - Efficient database queries
   - Proper connection pooling

4. **Error Handling**
   - Graceful degradation under load
   - Retry logic for transient failures
   - Comprehensive error responses
   - Circuit breaker pattern (bonus)

### Evaluation Criteria

- **Code Quality**: Clean, idiomatic Rust code
- **Architecture**: Scalable design decisions
- **Performance**: Efficient implementation
- **Testing**: Comprehensive test coverage
- **Documentation**: Clear code and API documentation

## Getting Started

### Prerequisites

- Rust 1.83+
- Docker & Docker Compose
- PostgreSQL and Redis (provided via docker-compose)
- [just](https://github.com/casey/just)

### Quick Setup (3 steps)

1. **Clone and prepare**:
   ```bash
   git clone <repository-url>
   cd api-rate-limiting-assesment
   cp .env.example .env  # Optional: modify if needed
   ```

2. **Start infrastructure**:
   ```bash
   just up       # Starts PostgreSQL + Redis
   just migrate  # Run database migrations
   ```

3. **Run tests**:
   ```bash
   just test-quick     # All integration tests (recommended)
   # or
   just test-validate  # Step-by-step with progress output
   ```

### Project Structure

```
├── libs/
│   ├── postgres_models/    # Database models and schema
│   └── redis_cache/        # Redis utilities
├── services/
│   └── api/               # API service
│       └── src/
│           ├── v1/        # API v1 endpoints
│           │   └── transactions/
│           │       └── submit.rs  # YOUR IMPLEMENTATION HERE
│           ├── extractors/  # Request validation
│           └── errors.rs    # Error handling
└── db/
    └── migrations/        # Database migrations
```

## Implementation Guide

### Test-Driven Implementation Approach

Your implementation should be **guided by the test suite** - implement features incrementally to make tests pass:

#### Phase 1: Core Functionality (Start Here)
**Goal**: Get basic transaction submission working
```bash
# Test to make pass first:
cargo test test_submit_transaction_success -- --exact
```
**Focus**: 
- Basic POST `/v1/transactions/submit` endpoint
- Request/response JSON handling  
- Database transaction creation
- UUID generation for transaction_id

#### Phase 2: Rate Limiting (Required)
**Goal**: Implement per-account rate limiting
```bash
# Tests to make pass:
cargo test test_basic_rate_limiting -- --exact
cargo test test_per_account_rate_limiting -- --exact
```
**Focus**:
- Redis-based sliding window rate limiting
- Account-specific rate limit tracking
- HTTP 429 responses with headers
- X-RateLimit-* headers in all responses

#### Phase 3: Queue Management (Required)
**Goal**: FIFO queue with priority support
```bash
# Tests to make pass:
cargo test test_sequential_queue_positions -- --exact
cargo test test_priority_queue_ordering -- --exact
```
**Focus**:
- Queue position calculation
- Priority handling (higher priority = better position)
- Estimated processing time calculation
- Redis queue operations

#### Phase 4: Performance (Critical)
**Goal**: Handle high concurrency
```bash
# Critical test to pass:
cargo test test_10k_concurrent_requests --release -- --ignored --nocapture
```
**Focus**:
- Thread safety and race condition prevention
- Database connection pooling optimization
- Redis connection pooling
- Sub-100ms p99 performance

### Main Implementation File

**`services/api/src/v1/transactions/submit.rs`** contains:
- Request/Response types
- Handler signature  
- **Detailed 6-step implementation guide with code examples**

### Available Resources

The codebase provides:
- Database models and connection handling (`libs/postgres_models/`)
- Redis connection pool and utilities (`libs/redis_cache/`)
- Error types and response handling (`services/api/src/errors.rs`)
- Comprehensive test infrastructure (49 tests total)

### Implementation Success Checklist

- [ ] **Phase 1**: Basic submission endpoint working
- [ ] **Phase 2**: Rate limiting (100 requests/minute per account)  
- [ ] **Phase 3**: Queue management with priority support
- [ ] **Phase 4**: Performance (10k concurrent, <100ms p99)
- [ ] **All integration tests pass** (`just test-validate`)
- [ ] **Load test passes** (`just load-test`)

## Testing

### Quick Start Testing

```bash
# 1. Setup environment (one time)
just test-setup   # Starts PostgreSQL + Redis, runs migrations

# 2. Start API (separate terminal)  
just run-dev      # Keep this running

# 3. Run tests
just test-validate    # All integration tests
just load-test        # Critical 10k performance test
just test-complete    # Everything
```

### Test Categories & Success Criteria

#### 1. Integration Tests (17 tests) - Must Pass 100%
```bash
just test-integration
```
**Validates**: Core API functionality, request/response handling, basic queue operations

#### 2. Rate Limiting Tests (8 tests) - Must Pass 100%  
```bash
just test-rate-limiting
```
**Validates**: 100/minute per account enforcement, HTTP headers, concurrent rate limiting

#### 3. Queue Management Tests (10 tests) - Must Pass 100%
```bash
just test-queue
```
**Validates**: FIFO with priority, position calculation, thread safety

#### 4. Performance Tests (6 tests) - Must Pass Critical Tests
```bash
just load-test        # CRITICAL: 10k concurrent, <100ms p99
just rate-limit-test  # CRITICAL: Rate limiting under load
```
**Success Criteria**:
- ✅ Handle 10,000 concurrent requests
- ✅ p99 latency <100ms  
- ✅ Success rate >99%
- ✅ Rate limiting works under load

#### 5. Edge Case Tests (8 tests) - Should Pass 80%+
```bash
just test-edge
```
**Validates**: Error handling, input validation, system robustness

### Test Development Workflow

```bash
# Development loop:
cargo test test_submit_transaction_success --exact  # Test specific feature
just test-validate                                  # Validate phase completion  
just load-test                                      # Validate performance
```

### Troubleshooting

**Environment Issues**:
- `just up` - Start infrastructure
- `just migrate` - Update database schema  
- `curl http://localhost:3000/health` - Check API

**Test Failures**:
- Database errors → Check PostgreSQL is running
- Rate limiting failures → Check Redis is running
- Performance failures → Run with `--release`, check system resources

## Submission Guidelines

### How to Submit

1. **Fork this repository** to your GitHub account
2. **Create a new branch** for your implementation (e.g., `solution-yourname`)
3. **Implement the solution** focusing on `services/api/src/v1/transactions/submit.rs`
4. **Push your branch** to your forked repository
5. **Share your work**:
   - **Option A**: Send us the link to your branch (if public)
   - **Option B**: If you prefer to keep your repository private, add `carlos@sqds.io` and `orion@sqds.io` as collaborators

### Time and Focus

1. **Time Limit**: Spend no more than 4-6 hours
2. **Focus Areas**:
   - Core functionality over polish
   - Performance over features
   - Tests for critical paths

3. **Include in Your Submission**:
   - Your implementation (primarily `submit.rs`)
   - Any additional tests you wrote
   - A brief `NOTES.md` file (2-3 paragraphs) covering:
     - Key design decisions
     - Trade-offs made
     - What you'd improve with more time

4. **Optional Enhancements** (if time permits):
   - Kafka integration rationale
   - Observability (metrics/tracing)
   - Advanced queue algorithms
   - Batch processing

## Resources

- [Axum Documentation](https://docs.rs/axum/latest/axum/)
- [Diesel Async Guide](https://github.com/weiznich/diesel_async)
- [Redis Commands](https://redis.io/commands)
- [bb8 Connection Pool](https://docs.rs/bb8/latest/bb8/)

## Questions?

If you have questions about the requirements, make reasonable assumptions and document them in your submission notes.

Good luck!