import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import './styles.css';

// Test alias imports
import { Button } from '@components/Button';

// Test define replacement
console.log('App version:', __APP_VERSION__);

// Test import.meta.env
console.log('Mode:', import.meta.env.MODE);
console.log('Dev:', import.meta.env.DEV);

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
    <Button>Click me</Button>
  </React.StrictMode>
);

// HMR API test
if (import.meta.hot) {
  import.meta.hot.accept();
  console.log('HMR enabled');
}
