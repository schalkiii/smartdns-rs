/** @type {import('next').NextConfig} */
const devConfig = {
  async rewrites() {
    return [
      {
        source: '/api/:path*',
        destination: 'http://127.0.0.1:8083/api/:path*',
      },
    ];
  },
};

const prodConfig = {
  output: 'export',
  assetPrefix: '/dashboard',
  images: {
    unoptimized: true,
  },
};

const config = process.env.NODE_ENV === 'production' ? prodConfig : devConfig;
config.typedRoutes = false;
export default config;