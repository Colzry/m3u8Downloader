import { openPath } from '@tauri-apps/plugin-opener';

export const openFolder = async (folderPath) => {
  try {
    await openPath(folderPath);
    console.log('文件夹已打开:', folderPath);
  } catch (error) {
    console.error('无法打开文件夹:', error);
  }
};