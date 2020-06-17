pipeline {
  agent any
  parameters {
    gitParameter(name: 'GIT_TAG',
                 type: 'PT_TAG',
                 description: 'The Git tag to checkout. If not specified "master" will be checkout.',
                 defaultValue: 'master')
    string(name: 'DOCKER_TAG',
           description: 'An extra Docker tag (e.g. "latest"). By default GIT_TAG will also be used as Docker tag')
  }

  stages {
    // Steps on UbuntuVM agent (where OCaml is installed)
    stage('Checkout Git TAG') {
      agent { label 'UbuntuVM' }
      steps {
        cleanWs()
        checkout([$class: 'GitSCM',
                  branches: [[name: "${params.GIT_TAG}"]],
                  doGenerateSubmoduleConfigurations: false,
                  extensions: [],
                  gitTool: 'Default',
                  submoduleCfg: [],
                  userRemoteConfigs: [[url: 'https://github.com/atolab/eclipse-zenoh.git']]
                ])
      }
    }
    stage('Setup opam dependencies') {
      agent { label 'UbuntuVM' }
      steps {
        sh '''
        git log --graph --date=short --pretty=tformat:'%ad - %h - %cn -%d %s' -n 20 || true
        OPAMJOBS=1 opam config report
        OPAMJOBS=1 opam install depext conf-libev
        OPAMJOBS=1 opam depext -yt
        OPAMJOBS=1 opam install -t . --deps-only
        '''
      }
    }
    stage('Build') {
      agent { label 'UbuntuVM' }
      steps {
        sh '''
        opam exec -- dune build @all
        '''
      }
    }
    stage('Tests') {
      agent { label 'UbuntuVM' }
      steps {
        sh '''
        opam exec -- dune runtest
        '''
      }
    }
    stage('Package') {
      agent { label 'UbuntuVM' }
      steps {
        sh '''
        cp -r _build/default/install eclipse-zenoh
        tar czvf eclipse-zenoh-${GIT_TAG}-Ubuntu-20.04-x64.tgz eclipse-zenoh/*/*.*
        '''
        stash includes: '_build/default/install/**, eclipse-zenoh-${GIT_TAG}-Ubuntu-20.04-x64.tgz', name: 'zenohPackage'
      }
    }

    // Steps on any agent (where cresentials are available)
    stage('Deploy to to download.eclipse.org') {
      steps {
        sshagent ( ['projects-storage.eclipse.org-bot-ssh']) {
          unstash 'zenohPackage'
          sh '''
          ssh genie.zenoh@projects-storage.eclipse.org mkdir -p /home/data/httpd/download.eclipse.org/zenoh/zenoh/${GIT_TAG}
          ssh genie.zenoh@projects-storage.eclipse.org ls -al /home/data/httpd/download.eclipse.org/zenoh/zenoh/${GIT_TAG}
          scp eclipse-zenoh-${GIT_TAG}-Ubuntu-20.04-x64.tgz  genie.zenoh@projects-storage.eclipse.org:/home/data/httpd/download.eclipse.org/zenoh/zenoh/${GIT_TAG}/
          '''
        }
      }
    }
    stage('Docker build') {
      steps {
        sh '''
        if [ -n "${DOCKER_TAG}" ]; then
          export EXTRA_TAG="-t eclipse/zenoh:${DOCKER_TAG}"
        fi
        echo docker build -t eclipse/zenoh:${GIT_TAG} ${EXTRA_TAG} .
        '''
      }
    }
    stage('Docker publish') {
      steps {
        withCredentials([usernamePassword(credentialsId: 'jenkins-docker-hub-creds',
            passwordVariable: 'DOCKER_HUB_CREDS_PSW', usernameVariable: 'DOCKER_HUB_CREDS_USR')])
        {
          sh '''
          echo "Login into docker as ${DOCKER_HUB_CREDS_USR}"
          echo docker login -u ${DOCKER_HUB_CREDS_USR} -p ${DOCKER_HUB_CREDS_PSW}
          echo docker push eclipse/zenoh .
          echo docker logout
          '''
        }
      }
    }
  }

  post {
    success {
        archiveArtifacts artifacts: 'eclipse-zenoh-${GIT_TAG}-Ubuntu-20.04-x64.tgz', fingerprint: true
    }
  }
}
