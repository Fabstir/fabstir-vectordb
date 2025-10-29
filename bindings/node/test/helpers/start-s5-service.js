// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

/**
 * Test Helper - Start S5 HTTP Service
 *
 * Programmatically starts the S5 HTTP service for tests.
 * Ensures the service is running before tests execute and cleans up after.
 *
 * Usage:
 *
 *   import { startS5Service } from './helpers/start-s5-service.js';
 *
 *   // In test setup
 *   let server;
 *   before(async () => {
 *     server = await startS5Service();
 *   });
 *
 *   // In test teardown
 *   after(async () => {
 *     await server.close();
 *   });
 */

import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import http from 'http';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

/**
 * Wait for service to be ready by polling health endpoint
 */
async function waitForServiceReady(port = 5522, timeoutMs = 10000) {
  const startTime = Date.now();
  const checkInterval = 100; // ms

  while (Date.now() - startTime < timeoutMs) {
    try {
      await new Promise((resolve, reject) => {
        const req = http.get(`http://localhost:${port}/health`, (res) => {
          if (res.statusCode === 200) {
            resolve();
          } else {
            reject(new Error(`Health check returned ${res.statusCode}`));
          }
        });

        req.on('error', reject);
        req.setTimeout(1000);
      });

      console.log(`✓ S5 service ready on port ${port}`);
      return true;
    } catch (error) {
      // Service not ready yet, wait and retry
      await new Promise(resolve => setTimeout(resolve, checkInterval));
    }
  }

  throw new Error(`S5 service did not become ready within ${timeoutMs}ms`);
}

/**
 * Start S5 HTTP service in mock mode for testing
 *
 * @param {Object} options - Configuration options
 * @param {number} options.port - Port number (default: 5522)
 * @param {string} options.mode - 'mock' or 'real' (default: 'mock')
 * @param {boolean} options.silent - Suppress service output (default: true)
 * @returns {Promise<Object>} Server handle with close() method
 */
export async function startS5Service(options = {}) {
  const {
    port = 5522,
    mode = 'mock',
    silent = true
  } = options;

  console.log(`Starting S5 service in ${mode} mode on port ${port}...`);

  // Path to service script
  const servicePath = join(__dirname, '../../services/s5-http-service.js');

  // Spawn service process
  const serviceProcess = spawn('node', [servicePath], {
    env: {
      ...process.env,
      S5_MODE: mode,
      S5_PORT: port.toString(),
      NODE_ENV: 'test'
    },
    stdio: silent ? 'pipe' : 'inherit'
  });

  // Capture output for debugging
  let stdout = '';
  let stderr = '';

  if (silent) {
    serviceProcess.stdout?.on('data', (data) => {
      stdout += data.toString();
    });

    serviceProcess.stderr?.on('data', (data) => {
      stderr += data.toString();
    });
  }

  // Handle process errors
  serviceProcess.on('error', (error) => {
    console.error('Failed to start S5 service:', error);
    throw error;
  });

  // Handle unexpected exit
  serviceProcess.on('exit', (code, signal) => {
    if (code !== 0 && code !== null) {
      console.error(`S5 service exited with code ${code}`);
      if (silent) {
        console.error('STDOUT:', stdout);
        console.error('STDERR:', stderr);
      }
    }
  });

  // Wait for service to be ready
  try {
    await waitForServiceReady(port);
  } catch (error) {
    // Service failed to start, kill process and show output
    serviceProcess.kill();
    if (silent) {
      console.error('Service output:');
      console.error('STDOUT:', stdout);
      console.error('STDERR:', stderr);
    }
    throw error;
  }

  // Return server handle
  return {
    process: serviceProcess,
    port,
    mode,

    /**
     * Stop the service
     */
    async close() {
      return new Promise((resolve) => {
        if (serviceProcess.killed) {
          resolve();
          return;
        }

        serviceProcess.on('exit', () => {
          console.log('✓ S5 service stopped');
          resolve();
        });

        serviceProcess.kill('SIGTERM');

        // Force kill if not stopped within 2 seconds
        setTimeout(() => {
          if (!serviceProcess.killed) {
            console.log('Force killing S5 service...');
            serviceProcess.kill('SIGKILL');
          }
        }, 2000);
      });
    },

    /**
     * Get service output (if silent mode)
     */
    getOutput() {
      return { stdout, stderr };
    }
  };
}

/**
 * Convenience function to start service before tests and stop after
 *
 * @param {Object} testContext - Test context object to attach server to
 * @param {Object} options - Service options
 */
export function useS5Service(testContext, options = {}) {
  let server;

  // Start service before tests
  before(async function() {
    this.timeout(15000); // Allow time for service startup
    server = await startS5Service(options);
    testContext.s5Service = server;
  });

  // Stop service after tests
  after(async function() {
    this.timeout(5000); // Allow time for cleanup
    if (server) {
      await server.close();
    }
  });
}
