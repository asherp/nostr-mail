FROM nginx:alpine

# Copy frontend files to nginx html directory
COPY . /usr/share/nginx/html

# Expose port 8080
EXPOSE 8080

# Override default nginx config to use port 8080
RUN echo 'server { \
    listen 8080; \
    server_name localhost; \
    root /usr/share/nginx/html; \
    index index.html; \
    location / { \
        try_files $uri $uri/ /index.html; \
    } \
}' > /etc/nginx/conf.d/default.conf

CMD ["nginx", "-g", "daemon off;"]
