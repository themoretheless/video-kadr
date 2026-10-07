-- Add critical indexes for performance optimization
-- Migration: 20261006_performance_indexes.sql

-- Index for jobs table - speeds up RUNNING job polling dramatically
CREATE INDEX IF NOT EXISTS idx_jobs_status_created ON jobs(status, created_at DESC);

-- Index for projects table - speeds up project lookup by video_id
CREATE UNIQUE INDEX IF NOT EXISTS idx_projects_video_id ON projects(video_id);

-- Index for library entries - speeds up filename lookups and sorting
CREATE INDEX IF NOT EXISTS idx_library_filename ON library(filename);
CREATE INDEX IF NOT EXISTS idx_library_created_at ON library(created_at DESC);

-- Composite index for library pagination (common pattern: ORDER BY created_at LIMIT/OFFSET)
CREATE INDEX IF NOT EXISTS idx_library_sorting ON library(created_at DESC, filename ASC);

-- Partial index for frequently queried job statuses (optimization for queue monitoring)
CREATE INDEX IF NOT EXISTS idx_jobs_running ON jobs(status, created_at) WHERE status IN ('Running', 'Queued');
