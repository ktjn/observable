{{/*
Renders a ClusterIP Service for a named Observable platform service.

Required context keys (passed as a dict via include):
  name     — service name (string)
  port     — service port number (int)
  Chart    — Helm .Chart object
  Release  — Helm .Release object
*/}}
{{- define "observable-common.service" -}}
apiVersion: v1
kind: Service
metadata:
  name: {{ .name }}
  labels:
    {{- include "observable-common.labels" . | nindent 4 }}
    app.kubernetes.io/component: {{ .name }}
spec:
  type: ClusterIP
  selector:
    {{- include "observable-common.selectorLabels" . | nindent 4 }}
  ports:
    - name: http
      port: {{ .port }}
      targetPort: http
      protocol: TCP
{{- end }}
