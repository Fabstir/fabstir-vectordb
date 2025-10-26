/**
 * CommonJS wrapper for S5 service test helper
 *
 * Provides simple start/stop functionality for tests using CommonJS.
 */

const { spawn } = require('child_process');
const http = require('http');
const path = require('path');

/**
 * Wait for service to be ready
 */
async function waitForServiceReady(port = 5522, timeoutMs = 10000) {
  const startTime = Date.now();
  const checkInterval = 100;

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
      await new Promise(resolve => setTimeout(resolve, checkInterval));
    }
  }

  throw new Error(`S5 service did not become ready within ${timeoutMs}ms`);
}

/**
 * Start S5 service for tests
 */
async function startS5Service(options = {}) {
  const { port = 5522, mode = 'mock', silent = true } = options;

  console.log(`Starting S5 service in ${mode} mode on port ${port}...`);

  const servicePath = path.join(__dirname, '../../services/s5-http-service.js');

  const serviceProcess = spawn('node', [servicePath], {
    env: {
      ...process.env,
      S5_MODE: mode,
      S5_PORT: port.toString(),
      NODE_ENV: 'test'
    },
    stdio: silent ? 'pipe' : 'inherit'
  });

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

  serviceProcess.on('error', (error) => {
    console.error('Failed to start S5 service:', error);
    throw error;
  });

  serviceProcess.on('exit', (code, signal) => {
    if (code !== 0 && code !== null) {
      console.error(`S5 service exited with code ${code}`);
      if (silent && (stdout || stderr)) {
        console.error('STDOUT:', stdout);
        console.error('STDERR:', stderr);
      }
    }
  });

  try {
    await waitForServiceReady(port);
  } catch (error) {
    serviceProcess.kill();
    if (silent && (stdout || stderr)) {
      console.error('Service output:');
      console.error('STDOUT:', stdout);
      console.error('STDERR:', stderr);
    }
    throw error;
  }

  return {
    process: serviceProcess,
    port,
    mode,

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

        setTimeout(() => {
          if (!serviceProcess.killed) {
            console.log('Force killing S5 service...');
            serviceProcess.kill('SIGKILL');
          }
        }, 2000);
      });
    },

    getOutput() {
      return { stdout, stderr };
    }
  };
}

module.exports = { startS5Service, waitForServiceReady };
