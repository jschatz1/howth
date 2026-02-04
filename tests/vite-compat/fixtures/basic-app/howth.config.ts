export default {
  resolve: {
    alias: {
      '@': '/src',
      '@components': '/src/components',
    },
  },
  define: {
    __APP_VERSION__: JSON.stringify('1.0.0'),
  },
  server: {
    port: 5174,
  },
};
