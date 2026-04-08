import * as core from '@actions/core';
import * as tc from '@actions/tool-cache';
import * as github from '@actions/github';
import * as exec from '@actions/exec';
import * as os from 'os';
import * as path from 'path';
import * as fs from 'fs';
import * as fsp from 'fs/promises';

async function run(): Promise<void> {
  try {
    const version = core.getInput('version');
    const token = core.getInput('github-token');
    let additionalArgs = core.getInput('args');

    const octokit = github.getOctokit(token);

    let releaseTag = version;
    if (version === 'latest') {
      const latestRelease = await octokit.rest.repos.getLatestRelease({
        owner: 'visiblelightio',
        repo: 'pnpm-catalog-lint',
      });
      releaseTag = latestRelease.data.tag_name;
    }

    const platform = os.platform();
    const arch = os.arch();

    const isMusl = (): boolean => {
      if (platform !== 'linux') {
        return false;
      }

      try {
        return (
          fs.existsSync('/lib/ld-musl-x86_64.so.1') ||
          fs.existsSync('/lib/ld-musl-aarch64.so.1')
        );
      } catch {
        return false;
      }
    };

    const platformTargets: Record<string, Record<string, string>> = {
      darwin: {
        arm64: 'aarch64-apple-darwin',
        x64: 'x86_64-apple-darwin',
      },
      win32: {
        arm64: 'aarch64-pc-windows-msvc',
        x64: 'x86_64-pc-windows-msvc',
      },
      linux: {
        arm64: isMusl()
          ? 'aarch64-unknown-linux-musl'
          : 'aarch64-unknown-linux-gnu',
        x64: isMusl()
          ? 'x86_64-unknown-linux-musl'
          : 'x86_64-unknown-linux-gnu',
      },
    };

    const platformTarget = platformTargets[platform]?.[arch];
    if (!platformTarget) {
      throw new Error(
        `Unsupported platform (${platform}) or architecture (${arch})`,
      );
    }

    const assetName = `pnpm-catalog-lint-${platformTarget}.zip`;

    const release = await octokit.rest.repos.getReleaseByTag({
      owner: 'visiblelightio',
      repo: 'pnpm-catalog-lint',
      tag: releaseTag,
    });

    const asset = release.data.assets.find((a) => a.name === assetName);
    if (!asset) {
      throw new Error(
        `Could not find asset ${assetName} in release ${releaseTag}`,
      );
    }

    core.info(
      `Downloading pnpm-catalog-lint ${releaseTag} for ${platformTarget}`,
    );
    const downloadPath = await tc.downloadTool(asset.browser_download_url);

    core.info('Extracting pnpm-catalog-lint binary...');
    const extractedPath = await tc.extractZip(downloadPath);

    const binaryName =
      platform === 'win32' ? 'pnpm-catalog-lint.exe' : 'pnpm-catalog-lint';
    const binaryPath = path.join(extractedPath, binaryName);

    if (platform !== 'win32') {
      await fsp.chmod(binaryPath, '755');
    }

    core.addPath(extractedPath);
    core.setOutput('pnpm-catalog-lint-path', binaryPath);
    core.info('pnpm-catalog-lint has been installed successfully');

    if (!additionalArgs) {
      additionalArgs = (await getArgsFromPackageJson()) || '';
    }
    const args = additionalArgs.split(' ').filter((arg) => arg !== '');

    const options: exec.ExecOptions = {
      ignoreReturnCode: true,
      env: {
        ...process.env,
        FORCE_COLOR: '3',
      },
    };

    // @actions/exec uses execFile internally (no shell injection risk)
    const exitCode = await exec.exec(binaryPath, args, options);

    if (exitCode !== 0) {
      throw new Error(
        `pnpm-catalog-lint execution failed with exit code ${exitCode}`,
      );
    }
  } catch (error) {
    if (error instanceof Error) {
      core.setFailed(error.message);
    } else {
      core.setFailed('An unexpected error occurred');
    }
  }
}

async function getArgsFromPackageJson(): Promise<string | undefined> {
  try {
    const packageJsonFile = await fsp.readFile(
      path.resolve(process.cwd(), 'package.json'),
    );
    const packageJson = JSON.parse(packageJsonFile.toString());

    const regexResult = /pnpm-catalog-lint\s([^&&]*)/g.exec(
      packageJson.scripts['pnpm-catalog-lint'],
    );
    if (regexResult && regexResult.length > 1) {
      const args = regexResult[1];
      core.info(`Using the arguments "${args}" from the root package.json`);
      return args;
    }
  } catch {
    core.info('Failed to extract args from package.json');
  }
}

run();
