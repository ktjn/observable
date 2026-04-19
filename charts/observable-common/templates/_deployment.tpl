{{/*
Renders a Deployment for an Observable platform service.

Required context keys (passed as a dict via include):
  name     — service name, used for metadata.name and selector labels (string)
  command  — binary name to execute as container command (string)
  port     — HTTP port the service listens on; 0 or absent means headless (int)
  service  — the .Values.services.<name> sub-tree (must contain replicas, resources)
  env      — list of {name, value} env var dicts
  Values   — root .Values (for global.image)
  Chart    — Helm .Chart object
  Release  — Helm .Release object

Probes are added automatically when port > 0; all services expose GET /health.
*/}}
{{- define "observable-common.deployment" -}}
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ .name }}
  labels:
    {{- include "observable-common.labels" . | nindent 4 }}
    app.kubernetes.io/component: {{ .name }}
spec:
  replicas: {{ .service.replicas | default 1 }}
  selector:
    matchLabels:
      {{- include "observable-common.selectorLabels" . | nindent 6 }}
  template:
    metadata:
      labels:
        {{- include "observable-common.labels" . | nindent 8 }}
        {{- include "observable-common.selectorLabels" . | nindent 8 }}
        app.kubernetes.io/component: {{ .name }}
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 65532
        runAsGroup: 65532
      containers:
        - name: {{ .name }}
          image: {{ include "observable-common.image" . }}
          imagePullPolicy: {{ .Values.global.image.pullPolicy | default "IfNotPresent" }}
          command: [{{ .command | quote }}]
          {{- if .port }}
          ports:
            - name: http
              containerPort: {{ .port }}
              protocol: TCP
          {{- end }}
          env:
            {{- range .env }}
            - name: {{ .name }}
              value: {{ .value | quote }}
            {{- end }}
          {{- if .port }}
          livenessProbe:
            httpGet:
              path: /health
              port: http
            initialDelaySeconds: 10
            periodSeconds: 10
            failureThreshold: 3
          readinessProbe:
            httpGet:
              path: /health
              port: http
            initialDelaySeconds: 5
            periodSeconds: 5
            failureThreshold: 3
          {{- end }}
          resources:
            {{- toYaml .service.resources | nindent 12 }}
{{- end }}
