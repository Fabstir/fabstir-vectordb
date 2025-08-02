#!/usr/bin/env node

/**
 * Real S5 Server - Bridge between Rust and real S5 network using Enhanced s5.js
 * 
 * This server provides HTTP endpoints that proxy to real S5 network operations.
 * It handles S5 authentication, portal registration, and data storage/retrieval.
 * 
 * Note: This is a simplified implementation. In production, you would:
 * - Support multiple identities/seed phrases via authentication headers
 * - Use proper CID-based storage instead of key mappings
 * - Implement proper S5 directory listings
 */

const express = require('express');
const bodyParser = require('body-parser');
const { S5 } = require('@parajbs-dev/s5client-js');
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

// S5 client instance
let s5Client = null;
let isInitialized = false;

// In-memory cache for better performance
const cache = new Map();

/**
 * Initialize S5 client with portal connection
 */
async function initializeS5() {
    if (isInitialized) return;

    try {
        console.log('Initializing S5 client...');
        
        // Create S5 client
        s5Client = new S5();

        // Use provided seed phrase or generate one
        let seedPhrase = SEED_PHRASE;
        if (!seedPhrase) {
            // Check if we have a saved seed phrase
            const seedFile = path.join(DATA_DIR, 'seed.txt');
            try {
                seedPhrase = await fs.readFile(seedFile, 'utf8');
                console.log('Loaded existing seed phrase');
            } catch {
                // Generate new seed phrase (simplified - in production use proper BIP39)
                const words = [];
                for (let i = 0; i < 12; i++) {
                    words.push(crypto.randomBytes(4).toString('hex'));
                }
                seedPhrase = words.join(' ');
                
                // Save for future use
                await fs.mkdir(DATA_DIR, { recursive: true });
                await fs.writeFile(seedFile, seedPhrase);
                console.log('Generated and saved new seed phrase');
            }
        }

        // Recover identity from seed phrase
        console.log('Recovering identity from seed phrase...');
        await s5Client.recoverIdentityFromSeedPhrase(seedPhrase);

        // Register on portal
        console.log(`Registering on portal: ${PORTAL_URL}`);
        await s5Client.registerOnNewPortal(PORTAL_URL);

        isInitialized = true;
        console.log('S5 client initialized successfully');
    } catch (error) {
        console.error('Failed to initialize S5 client:', error);
        throw error;
    }
}

/**
 * Ensure data directory exists
 */
async function ensureDataDir() {
    await fs.mkdir(DATA_DIR, { recursive: true });
}

// Health check endpoint
app.get('/health', (req, res) => {
    res.json({ 
        status: 'ok', 
        initialized: isInitialized,
        portal: PORTAL_URL,
        cache_size: cache.size
    });
});

// Store data
app.put('/s5/fs/:key(*)', async (req, res) => {
    try {
        await initializeS5();
        
        const key = req.params.key;
        const data = req.body;
        
        console.log(`PUT /s5/fs/${key} - ${data.length} bytes`);
        
        // Upload to S5 network
        // Enhanced s5.js handles the CID generation and storage
        const file = new File([data], key, { type: 'application/octet-stream' });
        const cid = await s5Client.uploadFile(file);
        
        // Store key->CID mapping locally for retrieval
        const mappingFile = path.join(DATA_DIR, 'mappings.json');
        let mappings = {};
        try {
            const content = await fs.readFile(mappingFile, 'utf8');
            mappings = JSON.parse(content);
        } catch {
            // File doesn't exist yet
        }
        
        mappings[key] = cid;
        await fs.writeFile(mappingFile, JSON.stringify(mappings, null, 2));
        
        // Update cache
        cache.set(key, data);
        
        res.json({ success: true, cid });
    } catch (error) {
        console.error('PUT error:', error);
        res.status(500).json({ error: error.message });
    }
});

// Retrieve data
app.get('/s5/fs/:key(*)', async (req, res) => {
    try {
        await initializeS5();
        
        const key = req.params.key;
        console.log(`GET /s5/fs/${key}`);
        
        // Check cache first
        if (cache.has(key)) {
            console.log('Cache hit');
            res.send(cache.get(key));
            return;
        }
        
        // Get CID from mapping
        const mappingFile = path.join(DATA_DIR, 'mappings.json');
        let mappings = {};
        try {
            const content = await fs.readFile(mappingFile, 'utf8');
            mappings = JSON.parse(content);
        } catch {
            res.status(404).json({ error: 'Mapping file not found' });
            return;
        }
        
        const cid = mappings[key];
        if (!cid) {
            res.status(404).json({ error: 'Key not found' });
            return;
        }
        
        // Download from S5 network
        const data = await s5Client.downloadData(cid);
        
        // Update cache
        cache.set(key, data);
        
        res.send(data);
    } catch (error) {
        console.error('GET error:', error);
        res.status(500).json({ error: error.message });
    }
});

// Delete data
app.delete('/s5/fs/:key(*)', async (req, res) => {
    try {
        await initializeS5();
        
        const key = req.params.key;
        console.log(`DELETE /s5/fs/${key}`);
        
        // Remove from mapping
        const mappingFile = path.join(DATA_DIR, 'mappings.json');
        let mappings = {};
        try {
            const content = await fs.readFile(mappingFile, 'utf8');
            mappings = JSON.parse(content);
        } catch {
            res.status(404).json({ error: 'Key not found' });
            return;
        }
        
        if (!mappings[key]) {
            res.status(404).json({ error: 'Key not found' });
            return;
        }
        
        delete mappings[key];
        await fs.writeFile(mappingFile, JSON.stringify(mappings, null, 2));
        
        // Remove from cache
        cache.delete(key);
        
        // Note: We can't delete from S5 network (immutable), 
        // but we've removed the mapping
        res.json({ success: true });
    } catch (error) {
        console.error('DELETE error:', error);
        res.status(500).json({ error: error.message });
    }
});

// Check if exists
app.head('/s5/fs/:key(*)', async (req, res) => {
    try {
        await initializeS5();
        
        const key = req.params.key;
        
        // Check mapping
        const mappingFile = path.join(DATA_DIR, 'mappings.json');
        let mappings = {};
        try {
            const content = await fs.readFile(mappingFile, 'utf8');
            mappings = JSON.parse(content);
        } catch {
            res.status(404).end();
            return;
        }
        
        if (mappings[key]) {
            res.status(200).end();
        } else {
            res.status(404).end();
        }
    } catch (error) {
        console.error('HEAD error:', error);
        res.status(500).end();
    }
});

// List keys (simple implementation)
app.get('/s5/fs/', async (req, res) => {
    try {
        await initializeS5();
        
        const mappingFile = path.join(DATA_DIR, 'mappings.json');
        let mappings = {};
        try {
            const content = await fs.readFile(mappingFile, 'utf8');
            mappings = JSON.parse(content);
        } catch {
            res.json([]);
            return;
        }
        
        const keys = Object.keys(mappings);
        res.json(keys);
    } catch (error) {
        console.error('LIST error:', error);
        res.status(500).json({ error: error.message });
    }
});

// Start server
async function start() {
    try {
        await ensureDataDir();
        await initializeS5();
        
        app.listen(PORT, () => {
            console.log(`Real S5 server listening on port ${PORT}`);
            console.log(`Portal URL: ${PORTAL_URL}`);
            console.log(`Data directory: ${DATA_DIR}`);
        });
    } catch (error) {
        console.error('Failed to start server:', error);
        process.exit(1);
    }
}

// Handle graceful shutdown
process.on('SIGINT', () => {
    console.log('\nShutting down...');
    process.exit(0);
});

// Start the server
start();