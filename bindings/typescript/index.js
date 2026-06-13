'use strict';
// Dynamically load the platform-specific native addon built by napi-rs.
const { existsSync, readdirSync } = require('fs');
const { join } = require('path');

const platforms = {
  'darwin arm64': 'kainetic.darwin-arm64.node',
  'darwin x64':   'kainetic.darwin-x64.node',
  'linux x64':    'kainetic.linux-x64-gnu.node',
  'win32 x64':    'kainetic.win32-x64-msvc.node',
};

const key = `${process.platform} ${process.arch}`;
const name = platforms[key];

if (!name) {
  throw new Error(`@kainetic/runtime: unsupported platform ${key}`);
}

const localPath = join(__dirname, name);
if (existsSync(localPath)) {
  module.exports = require(localPath);
} else {
  throw new Error(
    `@kainetic/runtime: native addon not found at ${localPath}. ` +
    'Run `npm run build` to compile it.'
  );
}
