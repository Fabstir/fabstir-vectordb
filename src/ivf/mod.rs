// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

pub mod core;
pub mod operations;
pub mod persistence;

pub use self::core::{Centroid, ClusterId, IVFConfig, IVFError, IVFIndex, TrainResult};

pub use self::persistence::{
    calculate_total_size, serialize_centroids, IVFMetadata, IVFPersister, IntegrityCheckResult,
    MigrationResult, PersistenceError, SerializableInvertedList,
};

pub use self::operations::{
    AddClustersResult, BalanceResult, BatchInsertResult, ClusterStats, CompactionResult,
    ExportedCentroid, MemoryUsage, OperationError, OptimizationResult, RetrainResult,
    SearchQuality,
};
