-- ==============================================================================
-- Jeeves Core - PostgreSQL Initialization
-- ==============================================================================
--
-- Creates the necessary extensions and base schema for jeeves-core
--
-- ==============================================================================

-- Enable pgvector extension for embeddings
CREATE EXTENSION IF NOT EXISTS vector;

-- Enable UUID generation
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Create schema for memory storage
CREATE SCHEMA IF NOT EXISTS memory;

-- Grant permissions
GRANT ALL PRIVILEGES ON SCHEMA memory TO assistant;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA memory TO assistant;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA memory TO assistant;

-- Log initialization
DO $$
BEGIN
    RAISE NOTICE 'Jeeves Core database initialized successfully';
    RAISE NOTICE 'Extensions: vector, uuid-ossp';
    RAISE NOTICE 'Schemas: public, memory';
END $$;
