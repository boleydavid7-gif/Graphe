import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import { installLamplightBridge } from './lamplightBridge';
import { installProairetosFilesBridge } from './proairetosFilesBridge';

installLamplightBridge();
installProairetosFilesBridge();

const root = document.getElementById('root');
if (!root) throw new Error('Writer could not find its application container.');
createRoot(root).render(<StrictMode><App /></StrictMode>);
