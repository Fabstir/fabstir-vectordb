/**
 * Schema Validation Tests for Vector DB Session
 * Tests the schema validation features in Node.js bindings
 */

const { describe, it, before, after } = require('node:test');
const assert = require('node:assert');
const { VectorDbSession } = require('../index.js');

describe('Schema Validation Tests', () => {
  let session;
  const testVectors = [];

  // Generate test vectors (384 dimensions for all-MiniLM-L6-v2)
  for (let i = 0; i < 10; i++) {
    const vector = Array(384).fill(0).map(() => Math.random());
    testVectors.push({
      id: `vec-${i}`,
      vector,
      metadata: {
        title: `Document ${i}`,
        views: i * 100,
        published: i % 2 === 0,
        tags: ['test', `tag${i}`],
      },
    });
  }

  before(async () => {
    console.log('# Creating session for schema validation tests...');
    session = await VectorDbSession.create({
      s5Portal: 'http://localhost:5522',
      userSeedPhrase: 'test-schema-seed',
      sessionId: `schema-test-${Date.now()}`,
      storageMode: 'mock',
    });
    console.log('# ✓ Session created');
  });

  after(async () => {
    if (session) {
      await session.destroy();
      console.log('# ✓ Session destroyed');
    }
  });

  it('should allow adding vectors without schema', async () => {
    await session.addVectors(testVectors.slice(0, 3));
    console.log('  ✓ Added 3 vectors without schema');
  });

  it('should set a valid schema', async () => {
    const schema = {
      fields: {
        title: 'String',
        views: 'Number',
        published: 'Boolean',
        tags: { Array: 'String' },
      },
      required: ['title', 'views'],
    };

    await session.setSchema(schema);
    console.log('  ✓ Schema set successfully');
  });

  it('should accept vectors matching schema', async () => {
    const validVector = {
      id: 'valid-vec',
      vector: Array(384).fill(0).map(() => Math.random()),
      metadata: {
        title: 'Valid Document',
        views: 500,
        published: true,
        tags: ['valid', 'test'],
      },
    };

    await session.addVectors([validVector]);
    console.log('  ✓ Accepted vector matching schema');
  });

  it('should reject vectors with missing required fields', async () => {
    const invalidVector = {
      id: 'invalid-vec-1',
      vector: Array(384).fill(0).map(() => Math.random()),
      metadata: {
        // Missing 'title' (required)
        views: 100,
        published: true,
      },
    };

    await assert.rejects(
      async () => {
        await session.addVectors([invalidVector]);
      },
      {
        name: 'Error',
        message: /Schema validation failed.*Missing required field: title/,
      }
    );
    console.log('  ✓ Rejected vector with missing required field');
  });

  it('should reject vectors with wrong field types', async () => {
    const invalidVector = {
      id: 'invalid-vec-2',
      vector: Array(384).fill(0).map(() => Math.random()),
      metadata: {
        title: 'Test',
        views: 'not-a-number', // Should be Number
        published: true,
      },
    };

    await assert.rejects(
      async () => {
        await session.addVectors([invalidVector]);
      },
      {
        name: 'Error',
        message: /Schema validation failed.*expected Number, found String/,
      }
    );
    console.log('  ✓ Rejected vector with wrong field type');
  });

  it('should allow optional fields to be omitted', async () => {
    const validVector = {
      id: 'optional-vec',
      vector: Array(384).fill(0).map(() => Math.random()),
      metadata: {
        title: 'Minimal Document',
        views: 50,
        // published and tags are optional
      },
    };

    await session.addVectors([validVector]);
    console.log('  ✓ Accepted vector with only required fields');
  });

  it('should allow null values for optional fields', async () => {
    const validVector = {
      id: 'null-vec',
      vector: Array(384).fill(0).map(() => Math.random()),
      metadata: {
        title: 'Null Test',
        views: 75,
        published: null, // Optional field with null
        tags: null, // Optional field with null
      },
    };

    await session.addVectors([validVector]);
    console.log('  ✓ Accepted vector with null optional fields');
  });

  it('should allow extra fields not in schema', async () => {
    const validVector = {
      id: 'extra-vec',
      vector: Array(384).fill(0).map(() => Math.random()),
      metadata: {
        title: 'Extra Fields Test',
        views: 200,
        published: true,
        extraField: 'This is allowed', // Not in schema
        anotherExtra: 123, // Also not in schema
      },
    };

    await session.addVectors([validVector]);
    console.log('  ✓ Accepted vector with extra fields');
  });

  it('should validate updateMetadata with schema', async () => {
    // Add a vector first
    const vec = {
      id: 'update-test-vec',
      vector: Array(384).fill(0).map(() => Math.random()),
      metadata: {
        title: 'Original Title',
        views: 100,
      },
    };
    await session.addVectors([vec]);

    // Valid update
    await session.updateMetadata('update-test-vec', {
      title: 'Updated Title',
      views: 200,
    });
    console.log('  ✓ Accepted valid metadata update');

    // Invalid update (wrong type)
    await assert.rejects(
      async () => {
        await session.updateMetadata('update-test-vec', {
          title: 123, // Should be String
          views: 300,
        });
      },
      {
        name: 'Error',
        message: /Schema validation failed.*expected String, found Number/,
      }
    );
    console.log('  ✓ Rejected invalid metadata update');
  });

  it('should clear schema when set to null', async () => {
    await session.setSchema(null);
    console.log('  ✓ Schema cleared');

    // Should now accept any metadata
    const anyVector = {
      id: 'any-vec',
      vector: Array(384).fill(0).map(() => Math.random()),
      metadata: {
        completely: 'random',
        fields: 12345,
      },
    };

    await session.addVectors([anyVector]);
    console.log('  ✓ Accepted vector after clearing schema');
  });

  it('should persist and load schema with saveToS5/loadUserVectors', async () => {
    // Set schema
    const schema = {
      fields: {
        name: 'String',
        score: 'Number',
      },
      required: ['name'],
    };
    await session.setSchema(schema);

    // Add some vectors
    const vectors = [
      {
        id: 'persist-1',
        vector: Array(384).fill(0).map(() => Math.random()),
        metadata: { name: 'Test 1', score: 95 },
      },
      {
        id: 'persist-2',
        vector: Array(384).fill(0).map(() => Math.random()),
        metadata: { name: 'Test 2', score: 87 },
      },
    ];
    await session.addVectors(vectors);

    // Save to S5
    const cid = await session.saveToS5();
    console.log(`  ✓ Saved with schema (CID: ${cid})`);

    // Destroy and recreate session
    await session.destroy();
    session = await VectorDbSession.create({
      s5Portal: 'http://localhost:5522',
      userSeedPhrase: 'test-schema-seed',
      sessionId: `schema-reload-${Date.now()}`,
      storageMode: 'mock',
    });

    // Load from S5
    await session.loadUserVectors(cid);
    console.log('  ✓ Loaded vectors with schema');

    // Schema should be enforced after loading
    const invalidVector = {
      id: 'after-load',
      vector: Array(384).fill(0).map(() => Math.random()),
      metadata: {
        score: 100, // Missing 'name' (required)
      },
    };

    await assert.rejects(
      async () => {
        await session.addVectors([invalidVector]);
      },
      {
        name: 'Error',
        message: /Schema validation failed.*Missing required field: name/,
      }
    );
    console.log('  ✓ Schema enforced after reload');
  });
});
