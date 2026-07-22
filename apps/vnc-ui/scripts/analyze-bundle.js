#!/usr/bin/env node

/**
 * Bundle size analysis script
 * Run: node scripts/analyze-bundle.js
 * 
 * Analyzes the build output and reports bundle sizes.
 */

import { readdir, stat, readFile } from 'fs/promises';
import { join, extname } from 'path';
import { gzipSync } from 'zlib';

const BUILD_DIR = join(process.cwd(), 'build');

const SIZE_THRESHOLDS = {
  js: { warning: 200 * 1024, error: 500 * 1024 },  // 200KB warning, 500KB error
  css: { warning: 50 * 1024, error: 100 * 1024 },    // 50KB warning, 100KB error
  wasm: { warning: 1 * 1024 * 1024, error: 5 * 1024 * 1024 },  // 1MB warning, 5MB error
  total: { warning: 1 * 1024 * 1024, error: 3 * 1024 * 1024 }  // 1MB warning, 3MB error
};

function formatSize(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function getSeverity(size, type) {
  const threshold = SIZE_THRESHOLDS[type] || SIZE_THRESHOLDS.total;
  if (size >= threshold.error) return 'error';
  if (size >= threshold.warning) return 'warning';
  return 'ok';
}

function getEmoji(severity) {
  switch (severity) {
    case 'error': return '❌';
    case 'warning': return '⚠️';
    default: return '✅';
  }
}

async function getFiles(dir, results = []) {
  const entries = await readdir(dir, { withFileTypes: true });
  
  for (const entry of entries) {
    const fullPath = join(dir, entry.name);
    
    if (entry.isDirectory()) {
      await getFiles(fullPath, results);
    } else {
      const fileStat = await stat(fullPath);
      const ext = extname(entry.name).toLowerCase().replace('.', '');
      
      let content = null;
      if (['js', 'css', 'html'].includes(ext)) {
        content = await readFile(fullPath);
      }
      
      results.push({
        path: fullPath.replace(BUILD_DIR, ''),
        size: fileStat.size,
        gzipSize: content ? gzipSync(content).length : null,
        type: ext
      });
    }
  }
  
  return results;
}

async function analyze() {
  console.log('📊 Bundle Size Analysis\n');
  console.log('='.repeat(60));
  
  try {
    const files = await getFiles(BUILD_DIR);
    
    // Group by type
    const byType = {};
    let totalSize = 0;
    let totalGzip = 0;
    
    for (const file of files) {
      if (!byType[file.type]) {
        byType[file.type] = [];
      }
      byType[file.type].push(file);
      totalSize += file.size;
      if (file.gzipSize) totalGzip += file.gzipSize;
    }
    
    // Report by type
    const typeOrder = ['js', 'css', 'wasm', 'bin', 'html', 'json', 'png', 'svg', 'ico'];
    
    for (const type of typeOrder) {
      const typeFiles = byType[type];
      if (!typeFiles || typeFiles.length === 0) continue;
      
      const typeSize = typeFiles.reduce((sum, f) => sum + f.size, 0);
      const typeGzip = typeFiles.reduce((sum, f) => sum + (f.gzipSize || 0), 0);
      const severity = getSeverity(typeSize, type);
      
      console.log(`\n${getEmoji(severity)} ${type.toUpperCase()} Files (${typeFiles.length} files)`);
      console.log('-'.repeat(40));
      
      // Sort by size descending
      typeFiles.sort((a, b) => b.size - a.size);
      
      for (const file of typeFiles.slice(0, 10)) {
        const gzipInfo = file.gzipSize ? ` (gzip: ${formatSize(file.gzipSize)})` : '';
        console.log(`  ${formatSize(file.size).padStart(12)}${gzipInfo.padStart(20)} ${file.path}`);
      }
      
      if (typeFiles.length > 10) {
        console.log(`  ... and ${typeFiles.length - 10} more files`);
      }
      
      console.log(`  ${'Total:'.padStart(12)} ${formatSize(typeSize)} (gzip: ${formatSize(typeGzip)})`);
    }
    
    // Total summary
    console.log('\n' + '='.repeat(60));
    console.log('📈 Summary');
    console.log('='.repeat(60));
    console.log(`Total size: ${formatSize(totalSize)}`);
    console.log(`Total gzip: ${formatSize(totalGzip)}`);
    console.log(`Files: ${files.length}`);
    
    // Overall severity
    const overallSeverity = getSeverity(totalSize, 'total');
    console.log(`\n${getEmoji(overallSeverity)} Overall: ${overallSeverity.toUpperCase()}`);
    
    // Warnings
    const warnings = [];
    for (const [type, typeFiles] of Object.entries(byType)) {
      const typeSize = typeFiles.reduce((sum, f) => sum + f.size, 0);
      if (getSeverity(typeSize, type) === 'warning') {
        warnings.push(`${type.toUpperCase()}: ${formatSize(typeSize)}`);
      }
      if (getSeverity(typeSize, type) === 'error') {
        warnings.push(`${type.toUpperCase()}: ${formatSize(typeSize)} (EXCEEDS LIMIT)`);
      }
    }
    
    if (warnings.length > 0) {
      console.log('\n⚠️ Warnings:');
      warnings.forEach(w => console.log(`  - ${w}`));
    }
    
    console.log('\n' + '='.repeat(60));
    
  } catch (error) {
    if (error.code === 'ENOENT') {
      console.error('❌ Build directory not found. Run "pnpm build" first.');
      process.exit(1);
    }
    throw error;
  }
}

analyze();
