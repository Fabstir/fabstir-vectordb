#!/usr/bin/env node

/**
 * Real S5 Server Mock - Simulates real S5 behavior for testing
 * 
 * This is a temporary mock that simulates what the real Enhanced s5.js
 * integration would do, allowing tests to pass without the actual S5 client.
 */

const express = require('express');
const bodyParser = require('body-parser');
const fs = require('fs').promises;
const path = require('path');
const crypto = require('crypto');

const app = express();
app.use(bodyParser.raw({ type: '*/*', limit: '50mb' }));

// Configuration
const PORT = process.env.S5_SERVICE_PORT || 5524;
const PORTAL_URL = process.env.S5_PORTAL_URL || 'https://s5.vup.cx';
const SEED_PHRASE = process.env.S5_SEED_PHRASE;
const DATA_DIR = process.env.S5_DATA_DIR || './.s5-real-data';

// In-memory storage (simulating S5 network storage)
const storage = new Map();
const cidMapping = new Map();

// Simulate CID generation
function generateCID(data) {
    const hash = crypto.createHash('sha256').update(data).digest('hex');
    return `baf${hash.substring(0, 40)}`; // Fake CID format
}

// Health check endpoint
app.get('/health', (req, res) => {
    res.json({ 
        status: 'ok', 
        mode: 'mock-real',
        portal: PORTAL_URL,
        storage_size: storage.size
    });
});

// Store data
app.put('/s5/fs/:key(*)', async (req, res) => {
    try {
        const key = req.params.key;
        const data = req.body;
        
        console.log(`PUT /s5/fs/${key} - ${data.length} bytes`);
        
        // Simulate S5 storage with CID
        const cid = generateCID(data);
        storage.set(cid, data);
        cidMapping.set(key, cid);
        
        // Simulate network delay
        await new Promise(resolve => setTimeout(resolve, 100 + Math.random() * 200));
        
        res.json({ success: true, cid });
    } catch (error) {
        console.error('PUT error:', error);
        res.status(500).json({ error: error.message });
    }
});

// Retrieve data
app.get('/s5/fs/:key(*)', async (req, res) => {
    try {
        const key = req.params.key;
        console.log(`GET /s5/fs/${key}`);
        
        const cid = cidMapping.get(key);
        if (!cid) {
            res.status(404).json({ error: 'Key not found' });
            return;
        }
        
        const data = storage.get(cid);
        if (!data) {
            res.status(404).json({ error: 'Data not found' });
            return;
        }
        
        // Simulate network delay
        await new Promise(resolve => setTimeout(resolve, 50 + Math.random() * 100));
        
        res.send(data);
    } catch (error) {
        console.error('GET error:', error);
        res.status(500).json({ error: error.message });
    }
});

// Delete data
app.delete('/s5/fs/:key(*)', async (req, res) => {
    try {
        const key = req.params.key;
        console.log(`DELETE /s5/fs/${key}`);
        
        const cid = cidMapping.get(key);
        if (!cid) {
            res.status(404).json({ error: 'Key not found' });
            return;
        }
        
        // Remove mapping (can't delete from "immutable" storage)
        cidMapping.delete(key);
        
        res.json({ success: true });
    } catch (error) {
        console.error('DELETE error:', error);
        res.status(500).json({ error: error.message });
    }
});

// Check if exists
app.head('/s5/fs/:key(*)', async (req, res) => {
    try {
        const key = req.params.key;
        
        if (cidMapping.has(key)) {
            res.status(200).end();
        } else {
            res.status(404).end();
        }
    } catch (error) {
        console.error('HEAD error:', error);
        res.status(500).end();
    }
});

// List keys
app.get('/s5/fs/', async (req, res) => {
    try {
        const keys = Array.from(cidMapping.keys());
        res.json(keys);
    } catch (error) {
        console.error('LIST error:', error);
        res.status(500).json({ error: error.message });
    }
});

// Start server
app.listen(PORT, () => {
    console.log(`Real S5 Mock Server listening on port ${PORT}`);
    console.log(`Portal URL (simulated): ${PORTAL_URL}`);
    console.log(`Seed phrase: ${SEED_PHRASE ? 'Provided' : 'Generated'}`);
});

// Handle graceful shutdown
process.on('SIGINT', () => {
    console.log('\nShutting down...');
    process.exit(0);
});